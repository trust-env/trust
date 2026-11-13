//! Loads a Trust project from its entry file: follows `include "..."`
//! (transitively, each file once), resolves every path a file writes
//! (`include`, `import`, `code { file include }`, `psql { seed }`) against
//! that file's own directory, rejects duplicate declarations, and reports
//! lex / parse errors as `file:line:column` with the offending line.

use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

use crate::{
    lexer::Lexer,
    parser::{Ast, InfraServiceType, ModuleKind, Parser, StorageType, TypeDeclaration},
};

/// `dir/path`, lexically normalized (`.` dropped, `..` applied); absolute
/// paths are kept.
pub fn resolve_path(dir: &Path, path: &str) -> String {
    let joined = if Path::new(path).is_absolute() { PathBuf::from(path) } else { dir.join(path) };
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let text = out.to_string_lossy().to_string();
    if text.is_empty() { ".".to_string() } else { text }
}

/// `path:line:col: message` plus the source line and a caret. `offset`
/// counts chars, like the lexer's spans.
pub fn located(path: &Path, source: &str, offset: usize, message: &str) -> String {
    let mut line = 1;
    let mut line_start = 0;
    for (i, c) in source.chars().enumerate() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let column = offset.saturating_sub(line_start) + 1;
    let text: String = source.chars().skip(line_start).take_while(|c| *c != '\n').collect();
    let gutter = " ".repeat(line.to_string().len());
    format!(
        "{}:{}:{}: {}\n{} |\n{} | {}\n{} | {}^",
        path.display(),
        line,
        column,
        message,
        gutter,
        line,
        text,
        gutter,
        " ".repeat(column - 1)
    )
}

/// Lexes and parses one file, resolving its paths against its directory.
/// `include`s stay in the result (see `load_project`).
pub fn parse_file(path: &Path, plugin_keywords: &[String]) -> Result<Vec<Ast>, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let tokens = Lexer::new(&source).run().map_err(|e| {
        let offset = regex::Regex::new(r"position (\d+)")
            .ok()
            .and_then(|re| re.captures(&e))
            .and_then(|c| c[1].parse().ok())
            .unwrap_or(0);
        located(path, &source, offset, &e)
    })?;
    let mut parser = Parser::new(tokens)
        .with_source(&source)
        .with_plugin_keywords(plugin_keywords.to_vec());
    let asts = parser
        .run()
        .map_err(|e| located(path, &source, parser.error_offset(), &clean_parse_error(&e)))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    Ok(asts
        .into_iter()
        .map(|ast| locate_logic(resolve_ast_paths(ast, dir), path, &source))
        .collect())
}

/// Records where each `logic {}` block starts, so its errors point into
/// the `.tr` file.
fn locate_logic(ast: Ast, path: &Path, source: &str) -> Ast {
    match ast {
        Ast::UseCase(mut use_case) => {
            if let Some(logic) = &mut use_case.logic {
                let (mut line, mut column) = (1, 1);
                for c in source.chars().take(logic.offset) {
                    if c == '\n' {
                        line += 1;
                        column = 1;
                    } else {
                        column += 1;
                    }
                }
                logic.file = path.display().to_string();
                logic.line = line;
                logic.column = column;
            }
            Ast::UseCase(use_case)
        }
        Ast::LogicLibrary(mut logic) => {
            place(&mut logic, path, source);
            Ast::LogicLibrary(logic)
        }
        other => other,
    }
}

/// Fills where `logic` starts in its file.
fn place(logic: &mut crate::parser::LogicSpec, path: &Path, source: &str) {
    let (mut line, mut column) = (1, 1);
    for c in source.chars().take(logic.offset) {
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    logic.file = path.display().to_string();
    logic.line = line;
    logic.column = column;
}

/// Drops the token dump the parser appends (the location says it better).
fn clean_parse_error(error: &str) -> String {
    let error = error.strip_prefix("Err ").unwrap_or(error);
    match error.find(": Unexpected token: ") {
        Some(i) => error[..i].to_string(),
        None => error.to_string(),
    }
}

/// A path written in a `.tr` file: relative to that file; for projects
/// written before that rule, a path that only exists relative to the
/// working directory is kept as is.
pub(crate) fn resolve_written_path(dir: &Path, path: &str) -> String {
    let resolved = resolve_path(dir, path);
    if !Path::new(&resolved).exists() && Path::new(path).exists() {
        return resolve_path(Path::new("."), path);
    }
    resolved
}

fn resolve_ast_paths(ast: Ast, dir: &Path) -> Ast {
    match ast {
        Ast::Include(path) => Ast::Include(resolve_written_path(dir, &path)),
        Ast::Import(mut import) => {
            if let ModuleKind::Path(p) = &import.module {
                import.module = ModuleKind::Path(PathBuf::from(resolve_written_path(dir, &p.to_string_lossy())));
            }
            Ast::Import(import)
        }
        Ast::UseCase(mut use_case) => {
            if let Some(code) = &mut use_case.code {
                code.file = resolve_written_path(dir, &code.file);
                code.includes = code.includes.iter().map(|p| resolve_written_path(dir, p)).collect();
            }
            Ast::UseCase(use_case)
        }
        Ast::Infra(mut infra) => {
            if let InfraServiceType::Storage(StorageType::Psql(psql)) = &mut infra.service_type {
                psql.seed = psql.seed.as_ref().map(|p| resolve_written_path(dir, p));
            }
            Ast::Infra(infra)
        }
        other => other,
    }
}

/// The kind and name of a declaration, for duplicate detection.
fn declaration_name(ast: &Ast) -> Option<(&'static str, String)> {
    match ast {
        Ast::TypeDecl(TypeDeclaration::Enum(e)) => Some(("enum", e.name.clone())),
        Ast::TypeDecl(t) => Some(("type", t.name().to_string())),
        Ast::Infra(i) => Some(("infra", i.name.clone())),
        Ast::UseCase(u) => Some(("use_case", u.name.clone())),
        Ast::Adapter(a) => Some(("adapter", a.name.clone())),
        Ast::App(a) => Some(("app", a.name.clone())),
        Ast::Schedule(s) => Some(("schedule", s.name.clone())),
        Ast::InfraSet(name, _) => Some(("infra", name.clone())),
        _ => None,
    }
}

/// `file:line` of the declaration of `name` in `path` (best effort).
fn declaration_site(path: &Path, name: &str) -> String {
    let source = std::fs::read_to_string(path).unwrap_or_default();
    let pattern = format!(r"\b(type|enum|infra|use_case|adapter|app|schedule|load_balancer)\b[\s:a-z_]*\b{}\b", regex::escape(name));
    let line = regex::Regex::new(&pattern)
        .ok()
        .and_then(|re| re.find(&source))
        .map(|m| source[..m.start()].matches('\n').count() + 1);
    match line {
        Some(line) => format!("{}:{}", path.display(), line),
        None => path.display().to_string(),
    }
}

/// Every declaration of the project rooted at `entry`, includes resolved.
pub fn load_project(entry: &Path, plugin_keywords: &[String]) -> Result<Vec<Ast>, String> {
    load_projects(&[entry.to_path_buf()], plugin_keywords)
}

/// Like `load_project`, with several entry files forming one project.
pub fn load_projects(entries: &[PathBuf], plugin_keywords: &[String]) -> Result<Vec<Ast>, String> {
    let mut loaded: Vec<PathBuf> = Vec::new();
    let mut asts: Vec<(PathBuf, Ast)> = Vec::new();
    let mut pending: Vec<PathBuf> = entries.iter().rev().cloned().collect();
    while let Some(path) = pending.pop() {
        let canonical = path.canonicalize().map_err(|e| format!("{}: {}", path.display(), e))?;
        if loaded.contains(&canonical) {
            continue;
        }
        loaded.push(canonical);
        let mut includes = Vec::new();
        for ast in parse_file(&path, plugin_keywords)? {
            match ast {
                Ast::Include(included) => includes.push(PathBuf::from(included)),
                other => asts.push((path.clone(), other)),
            }
        }
        // Depth-first, in the order the file lists them.
        pending.extend(includes.into_iter().rev());
    }

    let mut seen: HashMap<(&'static str, String), PathBuf> = HashMap::new();
    for (path, ast) in &asts {
        let Some(key) = declaration_name(ast) else { continue };
        // A type and an enum share one namespace.
        let namespace_key = if key.0 == "enum" { ("type", key.1.clone()) } else { key.clone() };
        if let Some(first) = seen.get(&namespace_key) {
            return Err(format!(
                "{} '{}' is declared twice: {} and {}",
                key.0,
                key.1,
                declaration_site(first, &key.1),
                declaration_site(path, &key.1)
            ));
        }
        seen.insert(namespace_key, path.clone());
    }
    Ok(asts.into_iter().map(|(_, ast)| ast).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_resolve_against_the_writing_file() {
        assert_eq!(resolve_path(Path::new("packages/api/modules/inbox"), "./capture.rs"), "packages/api/modules/inbox/capture.rs");
        assert_eq!(resolve_path(Path::new("a/b"), "../shared/x.rs"), "a/shared/x.rs");
        assert_eq!(resolve_path(Path::new("."), "./x.rs"), "x.rs");
        assert_eq!(resolve_path(Path::new("a"), "/abs/x.rs"), "/abs/x.rs");
    }

    #[test]
    fn locations_are_line_and_column() {
        let msg = located(Path::new("m.tr"), "type A {\n  x: #\n}", 14, "Unexpected character");
        assert!(msg.starts_with("m.tr:2:6: Unexpected character"), "{msg}");
        assert!(msg.contains("2 |   x: #\n  |      ^"), "{msg}");
    }

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn includes_are_followed_once_and_paths_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let entry = write(tmp.path(), "main.tr", "include \"./mods/a.tr\"\ninclude \"./mods/b.tr\"\n");
        write(tmp.path(), "mods/a.tr", "include \"./b.tr\"\ntype A { x: Int }\n");
        write(
            tmp.path(),
            "mods/b.tr",
            "type B { y: Int }\ntype In { a: Int }\ntype Out { b: Int }\nuse_case U { input: In output: Out code { file: \"./u.rs\" } }\n",
        );
        let asts = load_project(&entry, &[]).unwrap();
        assert_eq!(asts.len(), 5);
        let code_file = asts
            .iter()
            .find_map(|a| match a {
                Ast::UseCase(u) => u.code.as_ref().map(|c| c.file.clone()),
                _ => None,
            })
            .unwrap();
        assert!(code_file.ends_with("mods/u.rs"), "{code_file}");
    }

    #[test]
    fn duplicate_declarations_name_both_sites() {
        let tmp = tempfile::tempdir().unwrap();
        let entry = write(tmp.path(), "main.tr", "include \"./a.tr\"\n\ntype Task { x: Int }\n");
        write(tmp.path(), "a.tr", "// a\nenum Task { a b }\n");
        let err = load_project(&entry, &[]).unwrap_err();
        assert!(err.contains("'Task' is declared twice"), "{err}");
        assert!(err.contains("main.tr:3") && err.contains("a.tr:2"), "{err}");
    }

    #[test]
    fn parse_errors_point_into_the_included_file() {
        let tmp = tempfile::tempdir().unwrap();
        let entry = write(tmp.path(), "main.tr", "include \"./a.tr\"\n");
        write(tmp.path(), "a.tr", "type A {\n    x: Int\n    y Int\n}\n");
        let err = load_project(&entry, &[]).unwrap_err();
        assert!(err.contains("a.tr:3:"), "{err}");
    }
}
