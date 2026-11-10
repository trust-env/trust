pub mod span;
pub mod tokens;

pub use span::*;
pub use tokens::*;

pub struct Lexer {
    chars: Vec<char>,
    position: usize,
}

impl Lexer {
    pub fn new(input: &String) -> Self {
        let chars = input.chars().collect();

        Lexer { chars, position: 0 }
    }

    pub fn run(&mut self) -> Result<Vec<Spanned<Token>>, String> {
        self.position = 0;
        let mut tokens = Vec::new();
        let mut position = 0;

        while position < self.chars.len() {
            tokens.push(self.next_token()?);
            position = self.position;
        }
        Ok(tokens)
    }

    /// Skips whitespace and `// ...` line comments.
    fn skip_whitespace(&mut self) {
        loop {
            match (self.chars.get(self.position), self.chars.get(self.position + 1)) {
                (Some(c), _) if c.is_whitespace() => self.position += 1,
                (Some('/'), Some('/')) => {
                    while let Some(c) = self.chars.get(self.position) {
                        if *c == '\n' {
                            break;
                        }
                        self.position += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn get_word(&mut self) -> String {
        let start = self.position;

        while let Some(c) = self.chars.get(self.position) {
            if c.is_alphanumeric() || *c == '_' {
                self.position += 1;
            } else {
                break;
            }
        }
        self.chars[start..self.position].iter().collect()
    }

    fn reset(&mut self, position: usize) {
        self.position = position;
    }

    fn match_keyword(&mut self) -> Option<Keyword> {
        let w = self.get_word();

        match Keyword::try_from(w.clone()) {
            Ok(keyword) => Some(keyword),
            Err(_) => {
                self.reset(self.position - w.len());
                None
            }
        }
    }

    fn eof(&self) -> bool {
        self.position >= self.chars.len()
    }

    fn other_tokens(&mut self) -> Option<Token> {
        match self.chars.get(self.position) {
            Some('-') => {
                if let Some('>') = self.chars.get(self.position + 1) {
                    self.position += 2;
                    return Some(Token::Arrow);
                }
            }
            Some(':') => {
                self.position += 1;
                return Some(Token::Colon);
            }
            Some('.') => {
                self.position += 1;
                return Some(Token::Dot);
            }
            Some('{') => {
                self.position += 1;
                return Some(Token::OpenBrace);
            }
            Some('}') => {
                self.position += 1;
                return Some(Token::CloseBrace);
            }
            Some('(') => {
                self.position += 1;
                return Some(Token::OpenParen);
            }
            Some(')') => {
                self.position += 1;
                return Some(Token::CloseParen);
            }
            Some('=') => {
                self.position += 1;
                return Some(Token::Equal);
            }
            Some(',') => {
                self.position += 1;
                return Some(Token::Comma);
            }
            Some('<') => {
                self.position += 1;
                return Some(Token::OpenAngle);
            }
            Some('>') => {
                self.position += 1;
                return Some(Token::CloseAngle);
            }
            Some('!') => {
                self.position += 1;
                return Some(Token::Bang);
            }
            Some('+') => {
                self.position += 1;
                return Some(Token::Plus);
            }
            Some('[') => {
                self.position += 1;
                return Some(Token::OpenBracket);
            }
            Some(']') => {
                self.position += 1;
                return Some(Token::CloseBracket);
            }
            _ => {}
        }
        None
    }

    fn match_identifier(&mut self) -> Option<Token> {
        let word: String = self.get_word();

        if !word.is_empty() {
            return Some(Token::Identifier(word));
        }
        None
    }

    fn match_string_literal(&mut self) -> Result<Option<Spanned<Token>>, String> {
        let quote = match self.chars.get(self.position) {
            Some('"') => '"',
            Some('\'') => '\'',
            _ => return Ok(None),
        };

        let start = self.position;
        self.position += 1;

        while let Some(c) = self.chars.get(self.position) {
            if *c == quote {
                let value: String = self.chars[start + 1..self.position].iter().collect();
                self.position += 1;
                return Ok(Some(Spanned::new(
                    start,
                    self.position,
                    Token::StringLiteral(value),
                )));
            }
            self.position += 1;
        }

        Err(format!("Unterminated string literal starting at position {}", start))
    }

    fn next_token(&mut self) -> Result<Spanned<Token>, String> {
        self.skip_whitespace();

        if self.eof() {
            return Ok(Spanned::new(self.position, self.position, Token::Eof));
        }
        match self.match_keyword() {
            Some(keyword) => {
                return Ok(Spanned::new(
                    self.position - keyword.to_string().len(),
                    self.position,
                    Token::Keyword(keyword),
                ));
            }
            _ => {}
        };
        match self.match_identifier() {
            Some(token) => {
                return Ok(Spanned::new(
                    self.position - token.to_string().len(),
                    self.position,
                    token,
                ));
            }
            _ => {}
        };
        if let Some(token) = self.match_string_literal()? {
            return Ok(token);
        }
        match self.other_tokens() {
            Some(token) => {
                return Ok(Spanned::new(self.position - 1, self.position, token));
            }
            _ => {}
        }
        return Err(format!(
            "Unexpected character at position {}",
            self.position
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_comments_are_skipped() {
        let tokens = lex("// a comment\ntype // trailing\n// last").unwrap();
        assert_eq!(tokens[0], Token::Keyword(Keyword::Type));
        assert!(tokens[1..].iter().all(|t| *t == Token::Eof), "{:?}", tokens);
    }

    fn lex(src: &str) -> Result<Vec<Token>, String> {
        Lexer::new(&src.to_string())
            .run()
            .map(|spanned| spanned.into_iter().map(|s| s.value).collect())
    }

    #[test]
    fn lexes_every_keyword() {
        let cases: &[(&str, Keyword)] = &[
            ("adapter", Keyword::Adapter),
            ("infra", Keyword::Infra),
            ("use_case", Keyword::UseCase),
            ("storage", Keyword::Storage),
            ("psql", Keyword::Psql),
            ("String", Keyword::String),
            ("Uuid", Keyword::Uuid),
            ("Int", Keyword::Int),
            ("Float", Keyword::Float),
            ("Decimal", Keyword::Decimal),
            ("Bool", Keyword::Bool),
            ("DateTime", Keyword::DateTime),
            ("Option", Keyword::Option),
            ("Regex", Keyword::Regex),
            ("type", Keyword::Type),
            ("entity", Keyword::Entity),
            ("export", Keyword::Export),
            ("import", Keyword::Import),
            ("from", Keyword::From),
            ("input", Keyword::Input),
            ("output", Keyword::Output),
            ("call", Keyword::Call),
            ("http", Keyword::Http),
            ("routes", Keyword::Routes),
            ("on", Keyword::On),
            ("app", Keyword::App),
            ("run", Keyword::Run),
            ("errors", Keyword::Errors),
            ("status", Keyword::Status),
            ("body", Keyword::Body),
            ("auth", Keyword::Auth),
            ("load_balancer", Keyword::LoadBalancer),
            ("targets", Keyword::Targets),
            ("ai", Keyword::Ai),
            ("prompt", Keyword::Prompt),
            ("grpc", Keyword::Grpc),
            ("services", Keyword::Services),
            ("rpc", Keyword::Rpc),
            ("grpc_client", Keyword::GrpcClient),
            ("service", Keyword::Service),
            ("methods", Keyword::Methods),
            ("s3", Keyword::S3),
            ("bucket", Keyword::Bucket),
            ("blob", Keyword::Blob),
            ("filters", Keyword::Filters),
        ];
        for (src, expected) in cases {
            let tokens = lex(src).unwrap();
            assert_eq!(
                tokens.first(),
                Some(&Token::Keyword(expected.clone())),
                "expected '{}' to lex as {:?}",
                src,
                expected
            );
        }
    }

    #[test]
    fn double_and_single_quoted_strings_lex_identically() {
        let double = lex(r#""hello world""#).unwrap();
        let single = lex("'hello world'").unwrap();
        assert_eq!(double[0], Token::StringLiteral("hello world".to_string()));
        assert_eq!(single[0], Token::StringLiteral("hello world".to_string()));
    }

    #[test]
    fn unterminated_string_literal_is_a_clear_error() {
        let err = lex(r#""unterminated"#).unwrap_err();
        assert_eq!(err, "Unterminated string literal starting at position 0");
    }

    #[test]
    fn unexpected_character_is_a_clear_error() {
        // '#' is not a keyword, identifier char, quote, or recognized symbol.
        let err = lex("#").unwrap_err();
        assert_eq!(err, "Unexpected character at position 0");
    }

    #[test]
    fn integers_lex_as_plain_identifiers_not_a_numeric_token() {
        // There is no dedicated number token today — downstream parsers
        // `.parse::<u16>()` an Identifier. Pinning this so a future change
        // to add a real number token is a deliberate, visible break.
        let tokens = lex("42").unwrap();
        assert_eq!(tokens[0], Token::Identifier("42".to_string()));
    }

    #[test]
    fn regex_pattern_must_be_a_single_alnum_word() {
        // `Regex<slug>` — "slug" lexes as one Identifier between the angle
        // brackets, since only alphanumeric/`_` chars are word chars.
        let tokens = lex("Regex<slug>").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Keyword(Keyword::Regex),
                Token::OpenAngle,
                Token::Identifier("slug".to_string()),
                Token::CloseAngle,
            ]
        );
    }

    #[test]
    fn regex_pattern_with_metacharacters_does_not_lex_as_one_word() {
        // A real regex metacharacter (here `-`) breaks the single-word
        // assumption `Regex<pattern>` relies on — "slug-thing" lexes as two
        // separate identifiers with an unexpected `-` in between, since `-`
        // is only recognized as the first half of `->`. This pins the
        // current limitation (no real regex metacharacters are lexable
        // inside `Regex<...>`) so it's a visible, deliberate change if this
        // is ever lifted.
        let err = lex("Regex<slug-thing>").unwrap_err();
        assert_eq!(err, "Unexpected character at position 10");
    }

    #[test]
    fn symbols_lex_correctly() {
        let tokens = lex("-> : . { } ( ) = , < >").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Arrow,
                Token::Colon,
                Token::Dot,
                Token::OpenBrace,
                Token::CloseBrace,
                Token::OpenParen,
                Token::CloseParen,
                Token::Equal,
                Token::Comma,
                Token::OpenAngle,
                Token::CloseAngle,
            ]
        );
    }

    #[test]
    fn trailing_whitespace_produces_an_explicit_eof_token() {
        let tokens = lex("adapter\n").unwrap();
        assert_eq!(tokens.last(), Some(&Token::Eof));
    }

    #[test]
    fn no_trailing_whitespace_produces_no_explicit_eof_token() {
        // Quirk: `run()`'s loop only calls `next_token()` again (which is
        // what can produce `Token::Eof`) if there's still unconsumed input
        // (e.g. trailing whitespace) after the last real token. Source that
        // ends exactly on the last real token's last character never gets
        // an explicit Eof appended. This doesn't break the parser (its own
        // loop bounds on `tokens.len()`, not on finding an Eof), but it's a
        // surprising enough detail to pin explicitly.
        let tokens = lex("adapter").unwrap();
        assert_eq!(tokens.last(), Some(&Token::Keyword(Keyword::Adapter)));
        assert!(!tokens.contains(&Token::Eof));
    }
}
