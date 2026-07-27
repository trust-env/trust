enum TemplateSegment {
    Literal(String),
    Var(String),
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
            segments.push(TemplateSegment::Var(var));
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
        if let TemplateSegment::Var(name) = segment {
            if !vars.contains(&name) {
                vars.push(name);
            }
        }
    }
    vars
}

/// Turns a `"${HOST}:${PORT}"` style string into a Rust expression that
/// evaluates to a `String` at runtime by reading the referenced env vars.
/// A template with no `${...}` placeholders becomes a plain string literal.
pub fn to_rust_expr(template: &str) -> String {
    let segments = parse(template);

    if segments.iter().all(|s| matches!(s, TemplateSegment::Literal(_))) {
        let joined: String = segments
            .into_iter()
            .map(|s| match s {
                TemplateSegment::Literal(l) => l,
                TemplateSegment::Var(_) => unreachable!(),
            })
            .collect();
        return format!("{:?}.to_string()", joined);
    }

    let mut format_str = String::new();
    let mut args = Vec::new();

    for segment in segments {
        match segment {
            TemplateSegment::Literal(l) => {
                format_str.push_str(&l.replace('{', "{{").replace('}', "}}"))
            }
            TemplateSegment::Var(v) => {
                format_str.push_str("{}");
                args.push(format!(
                    "std::env::var({:?}).expect(\"{} environment variable not set\")",
                    v, v
                ));
            }
        }
    }
    format!("format!({:?}, {})", format_str, args.join(", "))
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
}
