use crate::lexer::Token;
use crate::parser::{Ast, Parser, PluginAst};

impl Parser {
    /// Recognizes `<plugin_keyword> { ... }`, where `<plugin_keyword>` is
    /// one of `self.plugin_keywords`. The body's contents are never
    /// tokenized as Trust syntax — it's scanned only for balanced braces,
    /// then the exact original source text between them is captured
    /// verbatim (preserving whitespace/quoting) for the owning plugin to
    /// parse however it likes.
    pub(super) fn try_plugin_construct(&mut self) -> Result<Ast, String> {
        let start = self.current;

        let name = match self.next_token() {
            Some(Token::Identifier(n)) if self.plugin_keywords.contains(&n) => n,
            _ => {
                self.reset(start);
                return Err("Expected a registered plugin keyword".to_string());
            }
        };

        match self.next_token() {
            Some(Token::OpenBrace) => {}
            _ => {
                self.reset(start);
                return Err(format!("Expected '{{' after plugin keyword '{}'", name));
            }
        }

        let body_start_idx = self.current;
        let mut depth = 1;
        loop {
            match self.next_token() {
                Some(Token::OpenBrace) => depth += 1,
                Some(Token::CloseBrace) => {
                    depth -= 1;
                    if depth == 0 {
                        let close_brace_idx = self.current - 1;
                        let raw_body = self.slice_source(body_start_idx, close_brace_idx - 1);
                        return Ok(Ast::Plugin(PluginAst { plugin_name: name, raw_body }));
                    }
                }
                Some(Token::Eof) | None => {
                    self.reset(start);
                    return Err(format!("Unterminated '{{' for plugin construct '{}'", name));
                }
                _ => {}
            }
        }
    }

    /// Reconstructs the original source text spanning tokens
    /// `[body_start_idx, body_end_idx]` (inclusive), using each token's char
    /// offsets rather than a lossy `Token::to_string()` join. Returns an
    /// empty string for an empty body (`body_start_idx > body_end_idx`) or
    /// if no source was ever provided via `with_source`.
    fn slice_source(&self, body_start_idx: usize, body_end_idx: usize) -> String {
        if self.source_chars.is_empty() || body_start_idx > body_end_idx {
            return String::new();
        }
        let start = self.tokens[body_start_idx].span.start;
        let end = self.tokens[body_end_idx].span.end;
        self.source_chars[start..end].iter().collect::<String>().trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    /// Mirrors the convention in `src/parser/import.rs` etc.: builds a
    /// `Parser` for direct, single-sub-parser testing. Full-file `.run()`
    /// integration is only used for the one test that checks a plugin
    /// construct coexists with a built-in one — the top-level `or_else`
    /// chain in `driver.rs` only ever surfaces its *last* candidate's error
    /// message on failure (a pre-existing, documented quirk, see
    /// `tests/e2e_negative.rs`), so every negative test here calls
    /// `try_plugin_construct` directly instead of going through `run()`.
    fn parser_for(src: &str, keywords: &[&str]) -> Parser {
        let tokens = Lexer::new(&src.to_string()).run().expect("lex should succeed");
        Parser::new(tokens).with_source(src).with_plugin_keywords(keywords.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn recognizes_a_registered_plugin_keyword() {
        let ast = parser_for(r#"note { message: "hello" }"#, &["note"]).try_plugin_construct().unwrap();
        match ast {
            Ast::Plugin(p) => {
                assert_eq!(p.plugin_name, "note");
                assert_eq!(p.raw_body, r#"message: "hello""#);
            }
            other => panic!("expected Ast::Plugin, got {:?}", other),
        }
    }

    #[test]
    fn empty_body_yields_an_empty_raw_body() {
        let ast = parser_for("note {}", &["note"]).try_plugin_construct().unwrap();
        match ast {
            Ast::Plugin(p) => assert_eq!(p.raw_body, ""),
            other => panic!("expected Ast::Plugin, got {:?}", other),
        }
    }

    #[test]
    fn nested_braces_inside_the_body_are_preserved_and_balanced() {
        let ast = parser_for("note { outer { inner } tail }", &["note"]).try_plugin_construct().unwrap();
        match ast {
            Ast::Plugin(p) => assert_eq!(p.raw_body, "outer { inner } tail"),
            other => panic!("expected Ast::Plugin, got {:?}", other),
        }
    }

    #[test]
    fn unterminated_plugin_block_is_a_clear_error() {
        let err = parser_for("note { message: \"hello\"", &["note"]).try_plugin_construct().unwrap_err();
        assert!(err.contains("Unterminated"), "got: {}", err);
    }

    #[test]
    fn an_unregistered_identifier_is_rejected() {
        // No plugin registered "gizmo" as a keyword, so this must be
        // rejected rather than silently accepted as a plugin construct.
        let err = parser_for("gizmo { message: \"hello\" }", &["note"]).try_plugin_construct().unwrap_err();
        assert!(err.contains("Expected a registered plugin keyword"), "got: {}", err);
    }

    #[test]
    fn plugin_construct_coexists_with_built_in_constructs() {
        let nodes = parser_for(
            r#"type UserId = Int
            note { message: "hello" }"#,
            &["note"],
        )
        .run()
        .unwrap();
        assert_eq!(nodes.len(), 2);
        assert!(matches!(nodes[0], Ast::TypeDecl(_)));
        assert!(matches!(nodes[1], Ast::Plugin(_)));
    }
}
