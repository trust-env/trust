use crate::{
    lexer::{Keyword, Token},
    parser::{AiSpec, AllowedType, Ast, ErrorMapping, Parser, UseCaseCall, UseCaseService},
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

    fn try_use_case_body(
        &mut self,
    ) -> Result<(Option<AllowedType>, Option<AllowedType>, Vec<UseCaseCall>, Option<AiSpec>), String> {
        let mut input = None;
        let mut output = None;
        let mut calls = Vec::new();
        let mut ai = None;
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
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse UseCase body".to_string());
                }
            }
        }
        Ok((input, output, calls, ai))
    }

    pub(super) fn try_use_case_service(&mut self) -> Result<Ast, String> {
        match (self.next_token(), self.next_token(), self.next_token()) {
            (
                Some(Token::Keyword(Keyword::UseCase)),
                Some(Token::Identifier(name)),
                Some(Token::OpenBrace),
            ) => {
                let (input, output, calls, ai) = self.try_use_case_body()?;
                if ai.is_some() && !calls.is_empty() {
                    return Err(format!(
                        "UseCase '{}' cannot declare both 'call' and 'ai' — they're mutually exclusive",
                        name
                    ));
                }
                Ok(Ast::UseCase(UseCaseService {
                    name: name.clone(),
                    input,
                    output,
                    calls,
                    ai,
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
            "UseCase 'Bad' cannot declare both 'call' and 'ai' — they're mutually exclusive"
        );
    }
}
