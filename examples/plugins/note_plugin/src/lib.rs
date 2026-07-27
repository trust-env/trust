//! A minimal reference plugin for the Trust compiler's `--plugin` mechanism,
//! used by `tests/e2e_plugin.rs` to prove the whole chain (dlopen -> keyword
//! recognition -> raw-body hand-off -> plugin parse -> plugin codegen ->
//! namespaced template render -> file written into the generated project)
//! works end to end.
//!
//! It registers a single top-level keyword, `note`, so that a `.tr` file
//! containing:
//!
//! ```text
//! note { message: "hello" }
//! ```
//!
//! generates `src/plugins/note.rs` containing a `pub const NOTE: &str =
//! "hello";` in the compiled project.
//!
//! This is not itself part of Trust's ABI contract — it is a plugin AUTHOR
//! implementing that contract from the outside, exactly as any other
//! language capable of emitting a C-ABI `.so` would.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{self, UnwindSafe};

use serde_json::Value;

/// Wraps `f` in `catch_unwind` and converts either its `Result` or an
/// unexpected panic into the `{"ok": ..., ...}` envelope every Trust plugin
/// call must return. Unwinding across an `extern "C"` boundary is UB, so
/// every exported function funnels through this.
fn catch_and_respond<F>(f: F) -> *mut c_char
where
    F: FnOnce() -> Result<Value, String> + UnwindSafe,
{
    let result = panic::catch_unwind(f).unwrap_or_else(|_| Err("note_plugin panicked".to_string()));
    let envelope = match result {
        Ok(value) => serde_json::json!({ "ok": true, "result": value }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    };
    CString::new(envelope.to_string())
        .unwrap_or_else(|_| CString::new(r#"{"ok":false,"error":"internal: NUL byte in plugin output"}"#).unwrap())
        .into_raw()
}

/// Reads a host-owned, null-terminated UTF-8 string. The caller must never
/// free `ptr` — it's owned by Trust, not this plugin.
///
/// # Safety
/// `ptr` must be null or point to a valid null-terminated C string that
/// outlives this call.
unsafe fn read_host_string(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("received a null string argument".to_string());
    }
    Ok(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
}

/// Extracts the quoted value of a `message: "..."` field out of the plugin
/// construct's raw body text. This is the plugin's own tiny hand-rolled
/// parser — Trust never interprets this text itself.
fn extract_message(body: &str) -> Result<String, String> {
    let after_key = body
        .split_once("message:")
        .map(|(_, rest)| rest.trim_start())
        .ok_or_else(|| "expected a 'message: \"...\"' field".to_string())?;
    let quote = after_key
        .chars()
        .next()
        .filter(|c| *c == '"' || *c == '\'')
        .ok_or_else(|| "expected 'message' value to be a quoted string".to_string())?;
    let rest = &after_key[1..];
    let end = rest.find(quote).ok_or_else(|| "unterminated 'message' string value".to_string())?;
    Ok(rest[..end].to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_plugin_abi_version() -> u32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_plugin_register() -> *mut c_char {
    catch_and_respond(|| {
        Ok(serde_json::json!({
            "plugin_name": "note_plugin",
            "keyword": "note",
            "templates": [
                {
                    "name": "note.rs.tera",
                    "content": "/// {{ message }}\npub const NOTE: &str = \"{{ message }}\";\n"
                }
            ]
        }))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_plugin_parse(raw_body: *const c_char) -> *mut c_char {
    catch_and_respond(|| {
        let body = unsafe { read_host_string(raw_body) }?;
        let message = extract_message(&body)?;
        Ok(serde_json::json!({ "message": message }))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn trust_plugin_codegen(node_json: *const c_char, _host_context_json: *const c_char) -> *mut c_char {
    catch_and_respond(|| {
        let text = unsafe { read_host_string(node_json) }?;
        let node: Value = serde_json::from_str(&text).map_err(|e| format!("invalid node JSON: {e}"))?;
        let message = node.get("message").and_then(Value::as_str).ok_or_else(|| "parsed node missing 'message'".to_string())?;
        Ok(serde_json::json!({
            "template_name": "note.rs.tera",
            "context": { "message": message },
            "output_file": "note.rs"
        }))
    })
}

/// Frees a string this plugin itself allocated (returned by `register`,
/// `parse`, or `codegen`). Trust never frees plugin memory any other way.
///
/// # Safety
/// `ptr` must be null or a pointer previously returned by one of this
/// plugin's own exported functions, not yet freed.
#[unsafe(no_mangle)]
pub extern "C" fn trust_plugin_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}
