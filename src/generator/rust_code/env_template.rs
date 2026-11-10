enum TemplateSegment {
    Literal(String),
    /// `${NAME}` or `${NAME:-default}`.
    Var(String, Option<String>),
}

fn var_segment(text: String) -> TemplateSegment {
    match text.split_once(":-") {
        Some((name, default)) => TemplateSegment::Var(name.to_string(), Some(default.to_string())),
        None => TemplateSegment::Var(text, None),
    }
}

fn parse(template: &str) -> Vec<TemplateSegment> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut var = String::new();
            for next in chars.by_ref() {
                if next == '}' {
                    break;
                }
                var.push(next);
            }
            if !literal.is_empty() {
                segments.push(TemplateSegment::Literal(std::mem::take(&mut literal)));
            }
            segments.push(var_segment(var));
        } else {
            literal.push(c);
        }
    }
    if !literal.is_empty() {
        segments.push(TemplateSegment::Literal(literal));
    }
    segments
}

/// The distinct `${VAR}` names referenced by a template, in the order they
/// first appear. Used to scaffold a `.env` file in `--dev` mode.
pub fn extract_vars(template: &str) -> Vec<String> {
    let mut vars = Vec::new();

    for segment in parse(template) {
        if let TemplateSegment::Var(name, _) = segment {
            if !vars.contains(&name) {
                vars.push(name);
            }
        }
    }
    vars
}

/// The `${NAME:-default}` default of `name` in `template`, if any.
pub fn var_default(template: &str, name: &str) -> Option<String> {
    parse(template).into_iter().find_map(|segment| match segment {
        TemplateSegment::Var(n, default) if n == name => default,
        _ => None,
    })
}

/// Turns a `"${HOST}:${PORT}"` style string into a Rust expression that
/// evaluates to a `String` at runtime by reading the referenced env vars.
/// A template with no `${...}` placeholders becomes a plain string literal.
pub fn to_rust_expr(template: &str) -> String {
    rust_expr(template, false)
}

/// Like `to_rust_expr`, but an unset variable without default reads as an
/// empty string instead of stopping the program — for values only needed
/// when used (an `http_client` never called, a shared secret: empty never
/// matches).
pub fn to_rust_expr_lenient(template: &str) -> String {
    rust_expr(template, true)
}

fn rust_expr(template: &str, lenient: bool) -> String {
    #[derive(serde::Serialize)]
    struct VarView {
        name: String,
        /// The variable name as a Rust string literal.
        literal: String,
        /// `${NAME:-default}`: the default as a Rust string literal.
        default_literal: Option<String>,
    }

    let segments = parse(template);
    let has_vars = segments.iter().any(|s| matches!(s, TemplateSegment::Var(..)));

    // Without placeholders the text is used as-is; with them it becomes a
    // `format!` string, so literal braces must be doubled.
    let mut text = String::new();
    let mut vars = Vec::new();
    for segment in segments {
        match segment {
            TemplateSegment::Literal(l) if has_vars => text.push_str(&l.replace('{', "{{").replace('}', "}}")),
            TemplateSegment::Literal(l) => text.push_str(&l),
            TemplateSegment::Var(v, default) => {
                text.push_str("{}");
                vars.push(VarView {
                    literal: format!("{:?}", v),
                    name: v,
                    default_literal: default.map(|d| format!("{:?}", d)),
                });
            }
        }
    }

    let mut context = tera::Context::new();
    context.insert("format_literal", &format!("{:?}", text));
    context.insert("vars", &vars);
    context.insert("lenient", &lenient);
    crate::generator::rust_code::render::render("env/rust_expr.rs.tera", &context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_vars_finds_all_placeholders_in_order() {
        let vars = extract_vars("${HOST}:${PORT}/${PATH}");
        assert_eq!(vars, vec!["HOST".to_string(), "PORT".to_string(), "PATH".to_string()]);
    }

    #[test]
    fn extract_vars_dedups_repeated_placeholders() {
        let vars = extract_vars("${HOST}:${PORT}@${HOST}");
        assert_eq!(vars, vec!["HOST".to_string(), "PORT".to_string()]);
    }

    #[test]
    fn extract_vars_on_literal_only_template_is_empty() {
        assert!(extract_vars("plain literal, no placeholders").is_empty());
    }

    #[test]
    fn to_rust_expr_on_literal_only_template_is_a_plain_string_literal() {
        assert_eq!(
            to_rust_expr("postgres://localhost"),
            "\"postgres://localhost\".to_string()"
        );
    }

    #[test]
    fn to_rust_expr_on_placeholder_containing_template_builds_a_format_call() {
        let expr = to_rust_expr("${HOST}:${PORT}");
        assert_eq!(
            expr,
            "format!(\"{}:{}\", std::env::var(\"HOST\").expect(\"HOST environment variable not set\"), std::env::var(\"PORT\").expect(\"PORT environment variable not set\"))"
        );
    }

    #[test]
    fn to_rust_expr_escapes_literal_braces_around_placeholders() {
        // A literal `{`/`}` next to a placeholder must be escaped to `{{`/`}}`
        // so the resulting `format!` call doesn't misparse it as another arg.
        let expr = to_rust_expr("{${HOST}}");
        assert_eq!(
            expr,
            "format!(\"{{{}}}\", std::env::var(\"HOST\").expect(\"HOST environment variable not set\"))"
        );
    }

    #[test]
    fn defaults_and_lenient_variables() {
        assert_eq!(
            to_rust_expr("${PORT:-8080}"),
            "format!(\"{}\", std::env::var(\"PORT\").unwrap_or_else(|_| \"8080\".to_string()))"
        );
        assert_eq!(extract_vars("${PORT:-8080}"), vec!["PORT".to_string()]);
        assert_eq!(var_default("x${PORT:-8080}", "PORT").as_deref(), Some("8080"));
        assert_eq!(
            to_rust_expr_lenient("${URL}"),
            "format!(\"{}\", std::env::var(\"URL\").unwrap_or_default())"
        );
    }
}
