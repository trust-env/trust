use crate::{
    lexer::Token,
    parser::{Ast, ConfigDeclaration, MapDeclaration, MapValue, Parser},
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
                Some(Token::Identifier(key)) if key == "tz" || key == "locale" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::StringLiteral(value))) => {
                        if key == "tz" {
                            config.tz = Some(value)
                        } else if value == "fr" || value == "en" {
                            config.locale = Some(value)
                        } else {
                            self.reset(start);
                            return Err(format!("config: unknown locale '{}' (known: \"fr\", \"en\")", value));
                        }
                    }
                    _ => {
                        self.reset(start);
                        return Err(format!("config: expected {}: \"...\"", key));
                    }
                },
                Some(Token::Identifier(key)) if key == "deps" => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        self.reset(start);
                        return Err("config: expected '{' after deps".to_string());
                    }
                    loop {
                        match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::Identifier(name)) | Some(Token::StringLiteral(name)) => {
                                match (self.next_token(), self.next_token()) {
                                    (Some(Token::Colon), Some(Token::StringLiteral(version))) => config.deps.push((name, version)),
                                    _ => {
                                        self.reset(start);
                                        return Err(format!("config: expected ': \"<version>\"' after dependency '{}'", name));
                                    }
                                }
                            }
                            other => {
                                self.reset(start);
                                return Err(format!("config: expected '<crate>: \"<version>\"' in deps, got {:?}", other));
                            }
                        }
                    }
                }
                other => {
                    self.reset(start);
                    return Err(format!("config: unexpected {:?} (known keys: json_case, deps, tz, locale)", other));
                }
            }
        }
    }
}

impl Parser {
    /// `map From -> To { field: path.to.field | literal }`.
    pub(super) fn try_map_declaration(&mut self) -> Result<Ast, String> {
        let start = self.current;
        let (from, to) = match (self.next_token(), self.next_token(), self.next_token(), self.next_token(), self.next_token()) {
            (
                Some(Token::Identifier(kw)),
                Some(Token::Identifier(from)),
                Some(Token::Arrow),
                Some(Token::Identifier(to)),
                Some(Token::OpenBrace),
            ) if kw == "map" => (from, to),
            _ => {
                self.reset(start);
                return Err("Expected 'map <From> -> <To> {'".to_string());
            }
        };
        let mut fields = Vec::new();
        loop {
            let field = match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(f)) => f,
                Some(Token::Keyword(k)) => k.to_string(),
                other => return Err(format!("map {} -> {}: expected a field name, got {:?}", from, to, other)),
            };
            if self.next_token() != Some(Token::Colon) {
                return Err(format!("map {} -> {}: expected ':' after '{}'", from, to, field));
            }
            let pos = self.current;
            let value = match self.next_token() {
                Some(Token::Identifier(word))
                    if !matches!(word.as_str(), "true" | "false" | "null")
                        && word.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') =>
                {
                    let mut path = vec![word];
                    loop {
                        let dot = self.current;
                        match (self.next_token(), self.next_token()) {
                            (Some(Token::Dot), Some(Token::Identifier(part))) => path.push(part),
                            _ => {
                                self.reset(dot);
                                break;
                            }
                        }
                    }
                    MapValue::Path(path)
                }
                _ => {
                    self.reset(pos);
                    MapValue::Literal(self.try_json_literal().map_err(|e| format!("map {} -> {}: field '{}': {}", from, to, field, e))?)
                }
            };
            fields.push((field, value));
        }
        Ok(Ast::Map(MapDeclaration { from, to, fields }))
    }
}
