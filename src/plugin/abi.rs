use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use serde_json::Value;

/// Calls a plugin FFI function that takes no string arguments and returns an
/// owned, envelope-wrapped JSON string (see `parse_envelope`). The returned
/// pointer is always released via `free_fn` before this function returns,
/// whether the call succeeded or not.
pub(crate) fn call_no_arg_json(
    call: unsafe extern "C" fn() -> *mut c_char,
    free_fn: unsafe extern "C" fn(*mut c_char),
) -> Result<Value, String> {
    let raw = unsafe { call() };
    read_and_free(raw, free_fn)
}

/// Same as `call_no_arg_json`, for a plugin function taking one host-owned
/// UTF-8 string argument.
pub(crate) fn call_one_arg_json(
    call: unsafe extern "C" fn(*const c_char) -> *mut c_char,
    free_fn: unsafe extern "C" fn(*mut c_char),
    arg: &str,
) -> Result<Value, String> {
    let c_arg = CString::new(arg).map_err(|e| format!("argument contains an interior NUL byte: {e}"))?;
    let raw = unsafe { call(c_arg.as_ptr()) };
    read_and_free(raw, free_fn)
}

/// Same as `call_no_arg_json`, for a plugin function taking two host-owned
/// UTF-8 string arguments.
pub(crate) fn call_two_arg_json(
    call: unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
    free_fn: unsafe extern "C" fn(*mut c_char),
    a: &str,
    b: &str,
) -> Result<Value, String> {
    let c_a = CString::new(a).map_err(|e| format!("first argument contains an interior NUL byte: {e}"))?;
    let c_b = CString::new(b).map_err(|e| format!("second argument contains an interior NUL byte: {e}"))?;
    let raw = unsafe { call(c_a.as_ptr(), c_b.as_ptr()) };
    read_and_free(raw, free_fn)
}

/// Reads a plugin-allocated, null-terminated UTF-8 string, releases it via
/// the plugin's own `free_fn` (never the host's allocator), and unwraps the
/// `{"ok": ..., ...}` envelope described in the plugin ABI.
fn read_and_free(raw: *mut c_char, free_fn: unsafe extern "C" fn(*mut c_char)) -> Result<Value, String> {
    if raw.is_null() {
        return Err("plugin function returned a null pointer".to_string());
    }
    let text = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
    unsafe { free_fn(raw) };
    parse_envelope(&text)
}

/// Parses the `{"ok": true, "result": ...}` / `{"ok": false, "error": "..."}`
/// envelope every plugin call returns, collapsing it into a plain `Result`.
pub(crate) fn parse_envelope(text: &str) -> Result<Value, String> {
    let parsed: Value = serde_json::from_str(text).map_err(|e| format!("plugin returned invalid JSON: {e} (raw: {text})"))?;
    let ok = parsed
        .get("ok")
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("plugin JSON envelope missing boolean 'ok' field (raw: {text})"))?;
    if ok {
        Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
    } else {
        let error = parsed.get("error").and_then(Value::as_str).unwrap_or("unknown plugin error").to_string();
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_envelope_unwraps_success() {
        let value = parse_envelope(r#"{"ok": true, "result": {"a": 1}}"#).unwrap();
        assert_eq!(value, serde_json::json!({"a": 1}));
    }

    #[test]
    fn parse_envelope_defaults_missing_result_to_null() {
        let value = parse_envelope(r#"{"ok": true}"#).unwrap();
        assert_eq!(value, Value::Null);
    }

    #[test]
    fn parse_envelope_surfaces_error_message() {
        let err = parse_envelope(r#"{"ok": false, "error": "boom"}"#).unwrap_err();
        assert_eq!(err, "boom");
    }

    #[test]
    fn parse_envelope_rejects_malformed_json() {
        let err = parse_envelope("not json").unwrap_err();
        assert!(err.contains("invalid JSON"));
    }

    #[test]
    fn parse_envelope_rejects_missing_ok_field() {
        let err = parse_envelope(r#"{"result": 1}"#).unwrap_err();
        assert!(err.contains("missing boolean 'ok'"));
    }
}
