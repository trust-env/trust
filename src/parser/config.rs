use crate::{
    lexer::Token,
    parser::{Ast, ConfigDeclaration, Parser},
};

impl Parser {
    /// `config { json_case: "camel" }` — `config` and its keys are
    /// contextual words.
    pub(super) fn try_config_declaration(&mut self) -> Result<Ast, String> {
        let start = self.current;
        match (self.next_token(), self.next_token()) {
            (Some(Token::Identifier(kw)), Some(Token::OpenBrace)) if kw == "config" => {}
            _ => {
                self.reset(start);
                return Err("Expected 'config {'".to_string());
            }
        }
        let mut config = ConfigDeclaration::default();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => return Ok(Ast::Config(config)),
                Some(Token::Comma) => continue,
                Some(Token::Identifier(key)) if key == "json_case" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(case)))
                            if case == "camel" || case == "snake" =>
                        {
                            config.json_case = Some(case)
                        }
                        _ => {
                            self.reset(start);
                            return Err("config: expected json_case: \"camel\" or \"snake\"".to_string());
                        }
                    }
                }
                other => {
                    self.reset(start);
                    return Err(format!("config: unexpected {:?} (known keys: json_case)", other));
                }
            }
        }
    }
}
