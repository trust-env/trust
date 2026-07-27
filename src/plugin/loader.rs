use std::collections::HashSet;
use std::os::raw::c_char;
use std::path::Path;

use libloading::Library;
use libloading::os::unix::Symbol as RawSymbol;
use serde_json::Value;

use crate::lexer::Keyword;
use crate::plugin::abi;

/// The only ABI version this compiler currently understands. Bumped
/// whenever the FFI contract in this module changes incompatibly.
const SUPPORTED_ABI_VERSION: u32 = 1;

type AbiVersionFn = unsafe extern "C" fn() -> u32;
type RegisterFn = unsafe extern "C" fn() -> *mut c_char;
type ParseFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;
type CodegenFn = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char;
type FreeFn = unsafe extern "C" fn(*mut c_char);

/// What a plugin's `trust_plugin_codegen` call resolves to: which template
/// to render, the data to render it with, and where to write the result
/// (a single path segment, relative to that plugin's generated-code folder).
pub struct PluginCodegenResult {
    pub template_name: String,
    pub context: Value,
    pub output_file: String,
}

/// A `.so` plugin loaded via `dlopen`, kept alive for the rest of the
/// process so its function pointers stay valid.
pub struct LoadedPlugin {
    pub name: String,
    pub keyword: String,
    _lib: Library,
    parse_fn: RawSymbol<ParseFn>,
    codegen_fn: RawSymbol<CodegenFn>,
    free_fn: RawSymbol<FreeFn>,
}

impl LoadedPlugin {
    pub fn parse(&self, raw_body: &str) -> Result<Value, String> {
        abi::call_one_arg_json(*self.parse_fn, *self.free_fn, raw_body)
            .map_err(|e| format!("plugin '{}': parse failed: {}", self.name, e))
    }

    pub fn codegen(&self, node: &Value, host_context: &Value) -> Result<PluginCodegenResult, String> {
        let node_json =
            serde_json::to_string(node).map_err(|e| format!("plugin '{}': failed to serialize parsed node: {}", self.name, e))?;
        let ctx_json = serde_json::to_string(host_context)
            .map_err(|e| format!("plugin '{}': failed to serialize host context: {}", self.name, e))?;
        let value = abi::call_two_arg_json(*self.codegen_fn, *self.free_fn, &node_json, &ctx_json)
            .map_err(|e| format!("plugin '{}': codegen failed: {}", self.name, e))?;

        let template_name = value
            .get("template_name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("plugin '{}': codegen result missing 'template_name'", self.name))?
            .to_string();
        let output_file = value
            .get("output_file")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("plugin '{}': codegen result missing 'output_file'", self.name))?
            .to_string();
        validate_output_file(&self.name, &output_file)?;
        let context = value.get("context").cloned().unwrap_or_else(|| Value::Object(Default::default()));

        Ok(PluginCodegenResult {
            template_name,
            context,
            output_file,
        })
    }
}

/// Rejects anything that isn't a single, safe path segment — a plugin must
/// not be able to write outside its own generated-code folder.
fn validate_output_file(plugin_name: &str, output_file: &str) -> Result<(), String> {
    if output_file.is_empty() || output_file.contains('/') || output_file.contains('\\') || output_file.contains("..") {
        return Err(format!(
            "plugin '{plugin_name}': codegen returned an unsafe output_file '{output_file}' (must be a single path segment, no '/' or '..')"
        ));
    }
    Ok(())
}

/// Loads a plugin `.so` from `path`, validates its declared ABI version and
/// keyword, and returns the loaded plugin plus its raw
/// `(template_name, content)` template pairs (the caller registers these
/// into the Tera template registry — this module doesn't know about Tera).
///
/// Never panics: every failure mode (missing library, missing symbol, ABI
/// mismatch, malformed manifest, keyword collision) surfaces as a plain
/// `Err(String)`.
pub fn load_plugin(path: &Path, already_registered_keywords: &HashSet<String>) -> Result<(LoadedPlugin, Vec<(String, String)>), String> {
    let lib = unsafe { Library::new(path) }.map_err(|e| format!("failed to load plugin '{}': {}", path.display(), e))?;

    let abi_version_fn = unsafe { lib.get::<AbiVersionFn>(b"trust_plugin_abi_version\0") }
        .map_err(|e| format!("plugin '{}' is missing required symbol 'trust_plugin_abi_version': {}", path.display(), e))?;
    let register_fn = unsafe { lib.get::<RegisterFn>(b"trust_plugin_register\0") }
        .map_err(|e| format!("plugin '{}' is missing required symbol 'trust_plugin_register': {}", path.display(), e))?;
    let parse_fn = unsafe { lib.get::<ParseFn>(b"trust_plugin_parse\0") }
        .map_err(|e| format!("plugin '{}' is missing required symbol 'trust_plugin_parse': {}", path.display(), e))?;
    let codegen_fn = unsafe { lib.get::<CodegenFn>(b"trust_plugin_codegen\0") }
        .map_err(|e| format!("plugin '{}' is missing required symbol 'trust_plugin_codegen': {}", path.display(), e))?;
    let free_fn = unsafe { lib.get::<FreeFn>(b"trust_plugin_free_string\0") }
        .map_err(|e| format!("plugin '{}' is missing required symbol 'trust_plugin_free_string': {}", path.display(), e))?;

    let version = unsafe { abi_version_fn() };
    if version != SUPPORTED_ABI_VERSION {
        return Err(format!(
            "plugin '{}' ABI version mismatch: compiler supports {}, plugin reports {}",
            path.display(),
            SUPPORTED_ABI_VERSION,
            version
        ));
    }

    let manifest =
        abi::call_no_arg_json(*register_fn, *free_fn).map_err(|e| format!("plugin '{}': register() failed: {}", path.display(), e))?;

    let plugin_name = manifest
        .get("plugin_name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("plugin '{}': manifest missing 'plugin_name'", path.display()))?
        .to_string();
    let keyword = manifest
        .get("keyword")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("plugin '{}': manifest missing 'keyword'", path.display()))?
        .to_string();

    validate_keyword(path, &keyword, already_registered_keywords)?;

    let templates: Vec<(String, String)> = manifest
        .get("templates")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("plugin '{}': manifest missing 'templates' array", path.display()))?
        .iter()
        .map(|t| -> Result<(String, String), String> {
            let name = t
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("plugin '{}': a template entry is missing 'name'", path.display()))?
                .to_string();
            let content = t
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("plugin '{}': a template entry is missing 'content'", path.display()))?
                .to_string();
            Ok((name, content))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Detach these three symbols from `lib`'s borrow so they can live
    // alongside it in one owned struct for the rest of the process.
    let parse_fn = unsafe { parse_fn.into_raw() };
    let codegen_fn = unsafe { codegen_fn.into_raw() };
    let free_fn = unsafe { free_fn.into_raw() };

    let plugin = LoadedPlugin {
        name: plugin_name,
        keyword,
        _lib: lib,
        parse_fn,
        codegen_fn,
        free_fn,
    };

    Ok((plugin, templates))
}

/// A plugin keyword must be non-empty, must not shadow a built-in keyword
/// (reusing `Keyword::try_from` as the single source of truth for "is this
/// word reserved"), and must not collide with a keyword another already-
/// loaded plugin registered.
fn validate_keyword(path: &Path, keyword: &str, already_registered_keywords: &HashSet<String>) -> Result<(), String> {
    if keyword.trim().is_empty() {
        return Err(format!("plugin '{}': declared keyword is empty", path.display()));
    }
    if Keyword::try_from(keyword.to_string()).is_ok() {
        return Err(format!("plugin '{}': keyword '{}' collides with a built-in keyword", path.display(), keyword));
    }
    if already_registered_keywords.contains(keyword) {
        return Err(format!(
            "plugin '{}': keyword '{}' is already registered by another --plugin",
            path.display(),
            keyword
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_keyword() {
        let err = validate_keyword(Path::new("x.so"), "", &HashSet::new()).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn rejects_builtin_keyword_collision() {
        let err = validate_keyword(Path::new("x.so"), "entity", &HashSet::new()).unwrap_err();
        assert!(err.contains("built-in"));
    }

    #[test]
    fn rejects_duplicate_plugin_keyword() {
        let mut seen = HashSet::new();
        seen.insert("note".to_string());
        let err = validate_keyword(Path::new("x.so"), "note", &seen).unwrap_err();
        assert!(err.contains("already registered"));
    }

    #[test]
    fn accepts_a_fresh_non_reserved_keyword() {
        assert!(validate_keyword(Path::new("x.so"), "note", &HashSet::new()).is_ok());
    }

    #[test]
    fn output_file_rejects_path_traversal() {
        assert!(validate_output_file("p", "../evil.rs").is_err());
    }

    #[test]
    fn output_file_rejects_nested_paths() {
        assert!(validate_output_file("p", "sub/evil.rs").is_err());
    }

    #[test]
    fn output_file_accepts_a_plain_filename() {
        assert!(validate_output_file("p", "note.rs").is_ok());
    }
}
