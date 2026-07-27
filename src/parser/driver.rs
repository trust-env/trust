use crate::{
    lexer::{Spanned, Token},
    parser::{Ast, Parser},
};

impl Parser {
    fn try_ast(&mut self) -> Result<Ast, String> {
        self.try_adapter_service()
            .or_else(|_| self.try_import_declaration())
            .or_else(|_| self.try_type_declaration())
            .or_else(|_| self.try_infra_service())
            .or_else(|_| self.try_use_case_service())
            .or_else(|_| self.try_plugin_construct())
            .or_else(|_| self.try_app_service())
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
