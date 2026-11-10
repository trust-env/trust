use crate::{
    lexer::{Keyword, Spanned, Token},
    parser::{Ast, Parser, TypeDeclaration},
};

impl Parser {
    /// Dispatches on the construct's leading word, so a malformed body
    /// reports that construct's own error (e.g. "'call' and 'ai' are
    /// mutually exclusive") rather than whichever alternative was tried last.
    fn try_ast(&mut self) -> Result<Ast, String> {
        let leading = self.tokens.get(self.current).map(|t| t.value.clone());
        match leading {
            Some(Token::Keyword(Keyword::Adapter)) => self.try_adapter_service(),
            Some(Token::Keyword(Keyword::Import)) => self.try_import_declaration(),
            Some(Token::Keyword(Keyword::Type)) => self.try_type_declaration(),
            Some(Token::Keyword(Keyword::Infra)) => self.try_infra_service(),
            Some(Token::Keyword(Keyword::UseCase)) => self.try_use_case_service(),
            Some(Token::Keyword(Keyword::App)) => self.try_app_service(),
            Some(Token::Identifier(word)) if word == "enum" => self.try_enum_declaration(),
            Some(Token::Identifier(word)) if word == "schedule" => self.try_schedule_declaration(),
            Some(Token::Identifier(word)) if word == "config" => self.try_config_declaration(),
            Some(Token::Identifier(word)) if word == "database" => self.try_database_declaration(),
            Some(Token::Identifier(word)) if word == "resource" => self.try_resource_declaration(),
            Some(Token::Identifier(word)) if word == "include" => {
                let start = self.current;
                self.next_token();
                match self.next_token() {
                    Some(Token::StringLiteral(path)) => Ok(Ast::Include(path)),
                    _ => {
                        self.reset(start);
                        Err("Expected a quoted path after 'include'".to_string())
                    }
                }
            }
            // `export type X { ... }` / `export enum E { ... }`: the
            // declaration is importable under its own name. On other
            // declarations `export` changes nothing (with `include`, every
            // declaration is visible anyway).
            Some(Token::Keyword(Keyword::Export)) => {
                self.next_token();
                let ast = self.try_ast()?;
                Ok(match ast {
                    Ast::TypeDecl(TypeDeclaration::Struct(mut s)) => {
                        s.export.get_or_insert_with(|| s.name.clone());
                        Ast::TypeDecl(TypeDeclaration::Struct(s))
                    }
                    Ast::TypeDecl(TypeDeclaration::Enum(mut e)) => {
                        e.export.get_or_insert_with(|| e.name.clone());
                        Ast::TypeDecl(TypeDeclaration::Enum(e))
                    }
                    Ast::TypeDecl(TypeDeclaration::Derived(mut d)) => {
                        d.export.get_or_insert_with(|| d.name.clone());
                        Ast::TypeDecl(TypeDeclaration::Derived(d))
                    }
                    other => other,
                })
            }
            Some(Token::Identifier(word)) if self.plugin_keywords.contains(&word) => {
                self.try_plugin_construct()
            }
            _ => Err("Expected a top-level declaration (type, enum, infra, use_case, adapter, app, schedule, config, database, resource, import, include)".to_string()),
        }
    }

    fn is_eof_tokens(&self) -> bool {
        match self.tokens.get(self.current) {
            Some(Spanned {
                value: Token::Eof, ..
            }) => true,
            _ => false,
        }
    }

    pub fn run(&mut self) -> Result<Vec<Ast>, String> {
        let mut ast_nodes = Vec::new();

        while self.current < self.tokens.len() {
            let position = self.current;

            if self.is_eof_tokens() {
                break;
            }
            match self.try_ast() {
                Ok(ast) => ast_nodes.push(ast),
                Err(e) => {
                    self.reset(position);
                    return Err(format!(
                        "Err {}: Unexpected token: {:?}",
                        e,
                        self.tokens.get(self.current)
                    ));
                }
            }
        }
        Ok(ast_nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn run(src: &str) -> Result<Vec<Ast>, String> {
        let tokens = Lexer::new(&src.to_string()).run().expect("lex should succeed");
        Parser::new(tokens).run()
    }

    #[test]
    fn empty_source_parses_to_no_ast_nodes() {
        assert!(run("").unwrap().is_empty());
    }

    #[test]
    fn whitespace_only_source_parses_to_no_ast_nodes() {
        // Trailing whitespace produces a leading Token::Eof (see lexer.rs's
        // `no_trailing_whitespace_produces_no_explicit_eof_token` test) which
        // `run()` must stop at immediately, not try to parse as an Ast node.
        assert!(run("   \n  ").unwrap().is_empty());
    }

    #[test]
    fn single_type_declaration_parses_to_one_ast_node() {
        let nodes = run("type UserId = Int").unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(matches!(nodes[0], Ast::TypeDecl(_)));
    }

    #[test]
    fn multiple_top_level_constructs_parse_in_order() {
        let nodes = run(
            r#"type UserId = Int
            import Foo from "./mod/foo.tr""#,
        )
        .unwrap();
        assert_eq!(nodes.len(), 2);
        assert!(matches!(nodes[0], Ast::TypeDecl(_)));
        assert!(matches!(nodes[1], Ast::Import(_)));
    }

    #[test]
    fn garbage_input_is_a_clear_unexpected_token_error() {
        // Lexable (plain identifiers) but not any valid top-level construct.
        let err = run("foo bar baz").unwrap_err();
        assert!(
            err.starts_with("Err "),
            "expected error to start with 'Err ', got: {}",
            err
        );
        assert!(
            err.contains("Unexpected token"),
            "expected error to mention 'Unexpected token', got: {}",
            err
        );
    }

    #[test]
    fn valid_prefix_followed_by_garbage_still_errors() {
        // A well-formed node followed by unparseable trailing tokens should
        // still surface the error rather than silently truncating output.
        let err = run("type UserId = Int foo bar baz").unwrap_err();
        assert!(err.contains("Unexpected token"));
    }
}
