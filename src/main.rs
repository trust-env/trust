use std::{collections::HashMap, collections::HashSet, path::PathBuf};

use trust::{
    ai::AiConfig,
    generator::CodeGen,
    generator::rust_code::render::register_plugin_templates,
    parser::Ast,
    plugin::{LoadedPlugin, load_plugin},
};

/// Splits `--key=value` CLI args into a flag map; anything else (including
/// bare `--dev`) is either a boolean flag or a `.tr` file path. This is how
/// every `ai {}` parameter (API key, provider, model, ...) reaches the
/// compiler — never hardcoded. Only the `--key=value` form is supported
/// (no space-separated values), so a plain positional-args pass can tell
/// files apart from flags unambiguously.
fn parse_flags(args: &[String]) -> HashMap<String, String> {
    let mut flags = HashMap::new();

    for arg in args {
        if let Some(rest) = arg.strip_prefix("--") {
            match rest.split_once('=') {
                Some((key, value)) => {
                    flags.insert(key.to_string(), value.to_string());
                }
                None => {
                    flags.insert(rest.to_string(), "true".to_string());
                }
            }
        }
    }
    flags
}

/// Collects every `--plugin=<path>` occurrence, in order. Unlike
/// `parse_flags` (a `HashMap`, one value per key), `--plugin` is repeatable,
/// so it gets its own dedicated scan.
fn collect_plugin_paths(args: &[String]) -> Vec<String> {
    args.iter().filter_map(|a| a.strip_prefix("--plugin=").map(str::to_string)).collect()
}

/// Loads every `--plugin`-specified `.so`, in order, rejecting any that fail
/// to load or whose declared keyword collides with a built-in or an
/// already-loaded plugin. Exits the process with a clear message on the
/// first failure, matching this CLI's existing lex/parse/codegen error
/// style.
fn load_plugins(plugin_paths: &[String]) -> Vec<LoadedPlugin> {
    let mut loaded_plugins = Vec::new();
    let mut seen_keywords = HashSet::new();
    let mut all_templates = Vec::new();

    for path in plugin_paths {
        match load_plugin(&PathBuf::from(path), &seen_keywords) {
            Ok((plugin, templates)) => {
                seen_keywords.insert(plugin.keyword.clone());
                for (name, content) in templates {
                    all_templates.push((format!("plugin/{}/{}", plugin.name, name), content));
                }
                loaded_plugins.push(plugin);
            }
            Err(e) => {
                eprintln!("Error occurred while loading plugin '{}': {}", path, e);
                std::process::exit(1);
            }
        }
    }

    if let Err(e) = register_plugin_templates(&all_templates) {
        eprintln!("Error occurred while registering plugin templates: {}", e);
        std::process::exit(1);
    }

    loaded_plugins
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<String>>();
    let dev_mode = args.iter().any(|a| a == "--dev");
    let flags = parse_flags(&args);
    let ai_config = AiConfig::from_flags(&flags);
    let loaded_plugins = load_plugins(&collect_plugin_paths(&args));
    let plugin_keywords: Vec<String> = loaded_plugins.iter().map(|p| p.keyword.clone()).collect();
    let files = args
        .into_iter()
        .filter(|a| !a.starts_with("--"))
        .collect::<Vec<String>>();
    if files.is_empty() {
        eprintln!("Usage: trust <file.tr> [more.tr ...] [--out=<dir>] [--dev] [--vendor=<dir>] [--verbose]");
        std::process::exit(1);
    }
    let verbose = flags.contains_key("verbose");
    let out_dir = PathBuf::from(flags.get("out").map(String::as_str).unwrap_or("./out"));
    let entries: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let asts = match trust::loader::load_projects(&entries, &plugin_keywords) {
        Ok(asts) => asts,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = CodeGen::new(asts.clone()).run_with_plugins(out_dir.clone(), dev_mode, &ai_config, &loaded_plugins) {
        eprintln!("Error occurred while generating code: {}", e);
        std::process::exit(1);
    }

    if verbose {
        eprintln!("Parsed ASTs: {:#?}", asts);
    }

    for (flag, kind) in [("emit-ts", "TypeScript client"), ("emit-openapi", "OpenAPI document")] {
        let Some(target) = flags.get(flag) else { continue };
        let all_asts: Vec<Ast> = asts.clone();
        let generator = CodeGen::new(Vec::new());
        let title = files.first().map(String::as_str).unwrap_or("api");
        let content = if flag == "emit-ts" {
            generator.emit_typescript(&all_asts)
        } else {
            generator.emit_openapi(&all_asts, title)
        };
        match content.and_then(|c| std::fs::write(target, c).map_err(|e| e.to_string())) {
            Ok(()) => println!("Wrote {} to {}", kind, target),
            Err(e) => {
                eprintln!("Error occurred while writing the {}: {}", kind, e);
                std::process::exit(1);
            }
        }
    }

    if let Some(vendor_dir) = flags.get("vendor") {
        // Bare `--vendor`: every crate from Cargo's git cache.
        let vendor_dir = if vendor_dir == "true" { PathBuf::from("/nonexistent") } else { PathBuf::from(vendor_dir) };
        match trust::vendor::vendor_dependencies(&out_dir, &vendor_dir) {
            Ok(vendored) if !vendored.is_empty() => {
                println!("Vendored {} into {}", vendored.join(", "), out_dir.join("vendor").display());
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error occurred while vendoring dependencies: {}", e);
                std::process::exit(1);
            }
        }
    }

    println!("Generated {}", out_dir.display());
}
