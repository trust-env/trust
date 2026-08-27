use crate::{
    lexer::{Keyword, Token},
    parser::{
        AiSpec, AllowedType, Ast, BlobCall, ErrorMapping, FilterMapping, Parser, UseCaseCall,
        UseCaseService,
    },
};

impl Parser {
    /// Parses `ai { infra: X prompt: "..." }`. `infra` is optional.
    fn try_ai_block(&mut self) -> Result<AiSpec, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Ai)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'ai {' block".to_string());
            }
        }

        let pos = self.current;
        let mut infra = None;
        let mut prompt = None;

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Infra)) => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::Identifier(name)) => infra = Some(name),
                        _ => {
                            self.reset(pos);
                            return Err("Expected identifier after 'infra:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'infra'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Prompt)) => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::StringLiteral(text)) => prompt = Some(text),
                        _ => {
                            self.reset(pos);
                            return Err("Expected string literal after 'prompt:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'prompt'".to_string());
                    }
                },
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse 'ai' block".to_string());
                }
            }
        }

        match prompt {
            Some(prompt) => Ok(AiSpec { infra, prompt }),
            None => {
                self.reset(pos);
                Err("'ai' block must declare 'prompt'".to_string())
            }
        }
    }

    /// Parses the block form of `call`: `call: { name: Target.method errors
    /// { ... } }`. Lets a single call declare its own error mapping,
    /// independent of any other call the same use case makes.
    fn try_use_case_call_block(&mut self) -> Result<UseCaseCall, String> {
        let pos = self.current;
        let mut target = None;
        let mut method = None;
        let mut errors = Vec::new();

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(s)) if s == "name" => match self.next_token() {
                    Some(Token::Colon) => match (self.next_token(), self.next_token(), self.next_token()) {
                        (Some(Token::Identifier(t)), Some(Token::Dot), Some(Token::Identifier(m))) => {
                            target = Some(t);
                            method = Some(m);
                        }
                        _ => {
                            self.reset(pos);
                            return Err("Expected '<Target>.<method>' after 'name:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'name'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Errors)) => {
                    self.revert(1);
                    errors = self.try_errors_block()?;
                }
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse 'call' block".to_string());
                }
            }
        }

        match (target, method) {
            (Some(target), Some(method)) => Ok(UseCaseCall { target, method, errors }),
            _ => {
                self.reset(pos);
                Err("'call' block must declare 'name: <Target>.<method>'".to_string())
            }
        }
    }

    fn try_use_case_call(&mut self) -> Result<UseCaseCall, String> {
        let pos = self.current;

        match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(target)), Some(Token::Dot), Some(Token::Identifier(method))) => {
                return Ok(UseCaseCall { target, method, errors: Vec::new() });
            }
            _ => self.reset(pos),
        }

        match self.next_token() {
            Some(Token::OpenBrace) => self.try_use_case_call_block(),
            _ => {
                self.revert(1);
                Err("Expected '<Target>.<method>' or '{' after 'call:'".to_string())
            }
        }
    }

    fn try_error_mapping(&mut self) -> Result<ErrorMapping, String> {
        let source = match self.next_token() {
            Some(Token::Identifier(s)) => s,
            _ => {
                self.revert(1);
                return Err("Expected source error identifier".to_string());
            }
        };
        match self.next_token() {
            Some(Token::Arrow) => {}
            _ => {
                self.revert(2);
                return Err("Expected '->' after source error identifier".to_string());
            }
        }
        let target = match self.next_token() {
            Some(Token::Identifier(t)) => t,
            _ => {
                self.revert(3);
                return Err("Expected target error identifier after '->'".to_string());
            }
        };

        let mut message = None;

        let pos = self.current;
        match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::OpenParen), Some(Token::StringLiteral(m)), Some(Token::CloseParen)) => {
                message = Some(m);
            }
            _ => self.reset(pos),
        }

        Ok(ErrorMapping {
            source,
            target,
            message,
        })
    }

    fn try_errors_block(&mut self) -> Result<Vec<ErrorMapping>, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Errors)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'errors {' block".to_string());
            }
        }

        let mut mappings = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                _ => {
                    self.revert(1);
                    mappings.push(self.try_error_mapping()?);
                }
            }
        }
        Ok(mappings)
    }

    /// Parses one `<field>: input.<input_field>` line inside `filters {}`.
    /// `input` lexes as `Keyword::Input` (it's a keyword everywhere else in
    /// the language too), not a plain identifier — so this matches the
    /// keyword token specifically, not `Token::Identifier("input")`.
    fn try_filter_mapping(&mut self) -> Result<FilterMapping, String> {
        let field = match self.next_token() {
            Some(Token::Identifier(f)) => f,
            _ => {
                self.revert(1);
                return Err("Expected field identifier in 'filters' block".to_string());
            }
        };
        match (
            self.next_token(),
            self.next_token(),
            self.next_token(),
            self.next_token(),
        ) {
            (
                Some(Token::Colon),
                Some(Token::Keyword(Keyword::Input)),
                Some(Token::Dot),
                Some(Token::Identifier(input_field)),
            ) => Ok(FilterMapping { field, input_field }),
            _ => {
                self.revert(5);
                Err(format!(
                    "Expected ': input.<field>' after '{}' in 'filters' block",
                    field
                ))
            }
        }
    }

    /// Parses `filters { <field>: input.<input_field> ... }`.
    fn try_filters_block(&mut self) -> Result<Vec<FilterMapping>, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Filters)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'filters {' block".to_string());
            }
        }

        let mut mappings = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                _ => {
                    self.revert(1);
                    mappings.push(self.try_filter_mapping()?);
                }
            }
        }
        Ok(mappings)
    }

    /// Parses `blob: Target.operation` — the bare `<Target>.<method>' form
    /// only (no block form, no `errors {}`: `blob {}` use cases get the
    /// same fixed `{NotFound, StorageError}` enum `ai {}` use cases do, see
    /// `BlobCall`'s doc comment).
    fn try_blob_call(&mut self) -> Result<BlobCall, String> {
        match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(target)), Some(Token::Dot), Some(Token::Identifier(operation))) => {
                Ok(BlobCall { target, operation })
            }
            _ => {
                self.revert(3);
                Err("Expected '<Target>.<operation>' after 'blob:'".to_string())
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn try_use_case_body(
        &mut self,
    ) -> Result<
        (
            Option<AllowedType>,
            Option<AllowedType>,
            Vec<UseCaseCall>,
            Option<AiSpec>,
            Option<BlobCall>,
            Vec<FilterMapping>,
        ),
        String,
    > {
        let mut input = None;
        let mut output = None;
        let mut calls = Vec::new();
        let mut ai = None;
        let mut blob = None;
        let mut filters = Vec::new();
        let pos = self.current;

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Input)) => match self.next_token() {
                    Some(Token::Colon) => input = Some(self.try_type()?),
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'input'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Output)) => match self.next_token() {
                    Some(Token::Colon) => output = Some(self.try_type()?),
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'output'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Call)) => match self.next_token() {
                    Some(Token::Colon) => calls.push(self.try_use_case_call()?),
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'call'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Ai)) => {
                    self.revert(1);
                    ai = Some(self.try_ai_block()?);
                }
                Some(Token::Keyword(Keyword::Blob)) => match self.next_token() {
                    Some(Token::Colon) => blob = Some(self.try_blob_call()?),
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'blob'".to_string());
                    }
                },
                Some(Token::Keyword(Keyword::Filters)) => {
                    self.revert(1);
                    filters = self.try_filters_block()?;
                }
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse UseCase body".to_string());
                }
            }
        }
        Ok((input, output, calls, ai, blob, filters))
    }

    pub(super) fn try_use_case_service(&mut self) -> Result<Ast, String> {
        match (self.next_token(), self.next_token(), self.next_token()) {
            (
                Some(Token::Keyword(Keyword::UseCase)),
                Some(Token::Identifier(name)),
                Some(Token::OpenBrace),
            ) => {
                let (input, output, calls, ai, blob, filters) = self.try_use_case_body()?;
                let modes_used = !calls.is_empty() as u8 + ai.is_some() as u8 + blob.is_some() as u8;
                if modes_used > 1 {
                    return Err(format!(
                        "UseCase '{}' cannot declare more than one of 'call', 'ai', 'blob' — they're mutually exclusive",
                        name
                    ));
                }
                if !filters.is_empty() && ai.is_some() {
                    return Err(format!(
                        "UseCase '{}': 'filters' cannot be combined with 'ai'",
                        name
                    ));
                }
                if !filters.is_empty() && blob.is_some() {
                    return Err(format!(
                        "UseCase '{}': 'filters' cannot be combined with 'blob'",
                        name
                    ));
                }
                Ok(Ast::UseCase(UseCaseService {
                    name: name.clone(),
                    input,
                    output,
                    calls,
                    ai,
                    blob,
                    filters,
                }))
            }
            _ => {
                self.revert(3);
                Err("Failed to parse UseCaseService".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parser_for(src: &str) -> Parser {
        let tokens = Lexer::new(&src.to_string()).run().expect("lex should succeed");
        Parser::new(tokens)
    }

    fn parse_use_case(src: &str) -> UseCaseService {
        match parser_for(src).try_use_case_service().unwrap() {
            Ast::UseCase(u) => u,
            other => panic!("expected Ast::UseCase, got {:?}", other),
        }
    }

    #[test]
    fn bare_call_form_parses_target_and_method() {
        let uc = parse_use_case(
            r#"use_case CreateUser {
                input: String
                output: String
                call: Storage.create
            }"#,
        );
        assert_eq!(uc.name, "CreateUser");
        assert_eq!(uc.input, Some(AllowedType::String));
        assert_eq!(uc.output, Some(AllowedType::String));
        assert_eq!(uc.calls.len(), 1);
        assert_eq!(uc.calls[0].target, "Storage");
        assert_eq!(uc.calls[0].method, "create");
        assert!(uc.calls[0].errors.is_empty());
        assert!(uc.ai.is_none());
    }

    #[test]
    fn block_call_form_parses_name_and_errors() {
        let uc = parse_use_case(
            r#"use_case CreateUser {
                call: {
                    name: Storage.create
                    errors {
                        NotFound -> Conflict("already exists")
                    }
                }
            }"#,
        );
        assert_eq!(uc.calls.len(), 1);
        assert_eq!(uc.calls[0].target, "Storage");
        assert_eq!(uc.calls[0].method, "create");
        assert_eq!(uc.calls[0].errors.len(), 1);
        assert_eq!(uc.calls[0].errors[0].source, "NotFound");
        assert_eq!(uc.calls[0].errors[0].target, "Conflict");
        assert_eq!(uc.calls[0].errors[0].message, Some("already exists".to_string()));
    }

    #[test]
    fn error_mapping_without_parenthesized_message_parses() {
        let uc = parse_use_case(
            r#"use_case CreateUser {
                call: {
                    name: Storage.create
                    errors {
                        NotFound -> Conflict
                    }
                }
            }"#,
        );
        assert_eq!(uc.calls[0].errors[0].message, None);
    }

    #[test]
    fn multiple_calls_preserve_declaration_order() {
        let uc = parse_use_case(
            r#"use_case CreateUser {
                call: Guard.check
                call: Storage.create
            }"#,
        );
        assert_eq!(uc.calls.len(), 2);
        assert_eq!(uc.calls[0].method, "check");
        assert_eq!(uc.calls[1].method, "create");
    }

    #[test]
    fn ai_block_alone_parses_with_optional_infra() {
        let uc = parse_use_case(
            r#"use_case SummarizeText {
                ai {
                    infra: Docs
                    prompt: "summarize the input text"
                }
            }"#,
        );
        assert!(uc.calls.is_empty());
        let ai = uc.ai.expect("expected an ai spec");
        assert_eq!(ai.infra, Some("Docs".to_string()));
        assert_eq!(ai.prompt, "summarize the input text");
    }

    #[test]
    fn ai_block_without_infra_parses() {
        let uc = parse_use_case(
            r#"use_case SummarizeText {
                ai {
                    prompt: "summarize the input text"
                }
            }"#,
        );
        assert_eq!(uc.ai.unwrap().infra, None);
    }

    #[test]
    fn ai_block_requires_prompt() {
        let src = r#"use_case SummarizeText {
            ai {
                infra: Docs
            }
        }"#;
        assert!(parser_for(src).try_use_case_service().is_err());
    }

    #[test]
    fn declaring_both_call_and_ai_is_rejected() {
        let src = r#"use_case Bad {
            call: Storage.create
            ai {
                prompt: "do something"
            }
        }"#;
        let err = parser_for(src).try_use_case_service().unwrap_err();
        assert_eq!(
            err,
            "UseCase 'Bad' cannot declare more than one of 'call', 'ai', 'blob' — they're mutually exclusive"
        );
    }

    #[test]
    fn bare_blob_form_parses_target_and_operation() {
        let uc = parse_use_case(
            r#"use_case RequestUploadUrl {
                blob: MediaInfra.presigned_put_url
            }"#,
        );
        assert!(uc.calls.is_empty());
        assert!(uc.ai.is_none());
        let blob = uc.blob.expect("expected a blob spec");
        assert_eq!(blob.target, "MediaInfra");
        assert_eq!(blob.operation, "presigned_put_url");
    }

    #[test]
    fn declaring_both_call_and_blob_is_rejected() {
        let src = r#"use_case Bad {
            call: Storage.create
            blob: MediaInfra.presigned_put_url
        }"#;
        assert!(parser_for(src).try_use_case_service().is_err());
    }

    #[test]
    fn declaring_both_ai_and_blob_is_rejected() {
        let src = r#"use_case Bad {
            ai {
                prompt: "do something"
            }
            blob: MediaInfra.presigned_put_url
        }"#;
        assert!(parser_for(src).try_use_case_service().is_err());
    }

    #[test]
    fn filters_block_parses_one_or_more_mappings() {
        let uc = parse_use_case(
            r#"use_case ListRatingsForSpotUseCase {
                call: RatingInfra.list
                filters {
                    spot_id: input.spot_id
                }
            }"#,
        );
        assert_eq!(uc.filters.len(), 1);
        assert_eq!(uc.filters[0].field, "spot_id");
        assert_eq!(uc.filters[0].input_field, "spot_id");
    }

    #[test]
    fn filters_block_can_declare_more_than_one_mapping() {
        let uc = parse_use_case(
            r#"use_case Search {
                call: MediaInfra.list
                filters {
                    spot_id: input.spot_id
                    kind: input.kind
                }
            }"#,
        );
        assert_eq!(uc.filters.len(), 2);
        assert_eq!(uc.filters[1].field, "kind");
        assert_eq!(uc.filters[1].input_field, "kind");
    }

    #[test]
    fn use_case_with_no_filters_block_has_an_empty_filters_vec() {
        let uc = parse_use_case(
            r#"use_case CreateUser {
                call: Storage.create
            }"#,
        );
        assert!(uc.filters.is_empty());
    }

    #[test]
    fn filters_requires_the_input_keyword_form() {
        // The RHS must be exactly `input.<field>` — a plain identifier (or
        // any other shape) is a clear parse error, not silently accepted.
        let src = r#"use_case Bad {
            call: RatingInfra.list
            filters {
                spot_id: somethingElse
            }
        }"#;
        assert!(parser_for(src).try_use_case_service().is_err());
    }

    #[test]
    fn filters_combined_with_ai_is_rejected() {
        let src = r#"use_case Bad {
            ai {
                prompt: "do something"
            }
            filters {
                spot_id: input.spot_id
            }
        }"#;
        let err = parser_for(src).try_use_case_service().unwrap_err();
        assert!(
            err.contains("'filters' cannot be combined with 'ai'"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn filters_combined_with_blob_is_rejected() {
        let src = r#"use_case Bad {
            blob: MediaInfra.presigned_put_url
            filters {
                spot_id: input.spot_id
            }
        }"#;
        let err = parser_for(src).try_use_case_service().unwrap_err();
        assert!(
            err.contains("'filters' cannot be combined with 'blob'"),
            "unexpected error: {}",
            err
        );
    }
}
