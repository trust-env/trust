use crate::{
    lexer::{Keyword, Token},
    parser::{AppService, Ast, LoadBalancerBinding, Parser},
};

impl Parser {
    /// Parses `targets { "host:port" ... }` — a static list of real network
    /// addresses (typically another named `app`'s own binary, deployed
    /// once per address on its own machine) that the load balancer's
    /// adapter-agnostic proxy forwards to.
    fn try_targets_block(&mut self) -> Result<Vec<String>, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Keyword(Keyword::Targets)), Some(Token::OpenBrace)) => {}
            _ => {
                self.revert(2);
                return Err("Expected 'targets {' block".to_string());
            }
        }

        let mut targets = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::StringLiteral(address)) => targets.push(address),
                _ => {
                    self.revert(1);
                    return Err("Expected a string literal address or '}' in 'targets' block".to_string());
                }
            }
        }
        Ok(targets)
    }

    /// Parses `load_balancer Name { targets { "host:port" ... } on: "..." }`.
    fn try_load_balancer_binding(&mut self) -> Result<LoadBalancerBinding, String> {
        let name = match (self.next_token(), self.next_token(), self.next_token()) {
            (
                Some(Token::Keyword(Keyword::LoadBalancer)),
                Some(Token::Identifier(name)),
                Some(Token::OpenBrace),
            ) => name,
            _ => {
                self.revert(3);
                return Err("Expected 'load_balancer <Name> {' block".to_string());
            }
        };

        let pos = self.current;
        let mut targets = None;
        let mut on = None;

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Targets)) => {
                    self.revert(1);
                    match self.try_targets_block() {
                        Ok(t) => targets = Some(t),
                        Err(e) => {
                            self.reset(pos);
                            return Err(e);
                        }
                    }
                }
                Some(Token::Keyword(Keyword::On)) => match self.next_token() {
                    Some(Token::Colon) => match self.next_token() {
                        Some(Token::StringLiteral(address)) => on = Some(address),
                        _ => {
                            self.reset(pos);
                            return Err("Expected string literal after 'on:'".to_string());
                        }
                    },
                    _ => {
                        self.reset(pos);
                        return Err("Expected ':' after 'on'".to_string());
                    }
                },
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse 'load_balancer' block".to_string());
                }
            }
        }

        match (targets, on) {
            (Some(targets), Some(on)) if !targets.is_empty() => {
                Ok(LoadBalancerBinding { name, targets, on })
            }
            _ => {
                self.reset(pos);
                Err("'load_balancer' must declare a non-empty 'targets' block and 'on'".to_string())
            }
        }
    }

    pub(super) fn try_app_service(&mut self) -> Result<Ast, String> {
        match (
            self.next_token(),
            self.next_token(),
            self.next_token(),
            self.next_token(),
            self.next_token(),
        ) {
            (
                Some(Token::Keyword(Keyword::App)),
                Some(Token::Identifier(name)),
                Some(Token::Colon),
                Some(Token::Keyword(Keyword::Run)),
                Some(Token::OpenBrace),
            ) => {
                let mut adapters = Vec::new();
                let mut load_balancers = Vec::new();

                loop {
                    match self.next_token() {
                        Some(Token::CloseBrace) => break,
                        Some(Token::Identifier(adapter_name)) => adapters.push(adapter_name),
                        Some(Token::Keyword(Keyword::LoadBalancer)) => {
                            self.revert(1);
                            load_balancers.push(self.try_load_balancer_binding()?);
                        }
                        _ => {
                            self.revert(1);
                            return Err("Expected adapter identifier or 'load_balancer' block in 'app:run' body".to_string());
                        }
                    }
                }
                Ok(Ast::App(AppService { name, adapters, load_balancers }))
            }
            _ => {
                self.revert(5);
                Err("Failed to parse AppService".to_string())
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

    fn parse_app(src: &str) -> AppService {
        match parser_for(src).try_app_service().unwrap() {
            Ast::App(a) => a,
            other => panic!("expected Ast::App, got {:?}", other),
        }
    }

    #[test]
    fn bare_adapter_identifiers_are_collected_in_order() {
        let app = parse_app("app Main: run { AdapterA AdapterB }");
        assert_eq!(app.name, "Main");
        assert_eq!(app.adapters, vec!["AdapterA".to_string(), "AdapterB".to_string()]);
        assert!(app.load_balancers.is_empty());
    }

    #[test]
    fn load_balancer_block_parses_all_fields() {
        let app = parse_app(
            r#"app Main: run {
                load_balancer LB {
                    targets {
                        "10.0.0.1:8080"
                        "10.0.0.2:8080"
                        "10.0.0.3:8080"
                    }
                    on: "0.0.0.0:9000"
                }
            }"#,
        );
        assert_eq!(app.name, "Main");
        assert!(app.adapters.is_empty());
        assert_eq!(app.load_balancers.len(), 1);
        let lb = &app.load_balancers[0];
        assert_eq!(lb.name, "LB");
        assert_eq!(
            lb.targets,
            vec!["10.0.0.1:8080".to_string(), "10.0.0.2:8080".to_string(), "10.0.0.3:8080".to_string()]
        );
        assert_eq!(lb.on, "0.0.0.0:9000");
    }

    #[test]
    fn bare_adapters_and_load_balancers_can_be_mixed() {
        let app = parse_app(
            r#"app Main: run {
                AdapterA
                load_balancer LB {
                    targets {
                        "10.0.0.1:8080"
                    }
                    on: "0.0.0.0:9000"
                }
            }"#,
        );
        assert_eq!(app.name, "Main");
        assert_eq!(app.adapters, vec!["AdapterA".to_string()]);
        assert_eq!(app.load_balancers.len(), 1);
    }

    #[test]
    fn load_balancer_requires_targets_and_on() {
        let src = r#"app Main: run {
            load_balancer LB {
                targets {
                    "10.0.0.1:8080"
                }
            }
        }"#;
        assert!(parser_for(src).try_app_service().is_err());
    }

    #[test]
    fn load_balancer_rejects_an_empty_targets_block() {
        let src = r#"app Main: run {
            load_balancer LB {
                targets { }
                on: "0.0.0.0:9000"
            }
        }"#;
        assert!(parser_for(src).try_app_service().is_err());
    }

    #[test]
    fn missing_app_run_prefix_is_an_error() {
        let err = parser_for("app Main: walk { AdapterA }").try_app_service().unwrap_err();
        assert_eq!(err, "Failed to parse AppService");
    }
}
