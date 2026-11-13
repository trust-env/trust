use crate::{
    lexer::{Keyword, Token},
    parser::{Ast, MapValue, MirrorDeclaration, MirrorScope, Parser},
};

impl Parser {
    /// `mirror Name { source: X into: Infra key: [a, b] scope { ... } map { ... } window { ... } }`.
    pub(super) fn try_mirror_declaration(&mut self) -> Result<Ast, String> {
        let start = self.current;
        let name = match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(kw)), Some(Token::Identifier(name)), Some(Token::OpenBrace)) if kw == "mirror" => name,
            _ => {
                self.reset(start);
                return Err("Expected 'mirror <Name> {'".to_string());
            }
        };
        let err = |what: &str| format!("mirror '{}': {}", name, what);
        let mut mirror = MirrorDeclaration {
            name: name.clone(),
            source: String::new(),
            into: String::new(),
            key: Vec::new(),
            scope: Vec::new(),
            map: Vec::new(),
            window_from: "today".to_string(),
            window_zone: None,
            window_days: 28,
            skip: Vec::new(),
        };
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(k)) if k == "source" || k == "into" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => {
                        if k == "source" {
                            mirror.source = v
                        } else {
                            mirror.into = v
                        }
                    }
                    _ => return Err(err(&format!("expected {}: <Name>", k))),
                },
                Some(Token::Identifier(k)) if k == "key" => {
                    if (self.next_token(), self.next_token()) != (Some(Token::Colon), Some(Token::OpenBracket)) {
                        return Err(err("expected key: [field, ...]"));
                    }
                    loop {
                        match self.next_token() {
                            Some(Token::CloseBracket) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::Identifier(f)) => mirror.key.push(f),
                            other => return Err(err(&format!("unexpected {:?} in key", other))),
                        }
                    }
                }
                Some(Token::Identifier(k)) if k == "scope" => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        return Err(err("expected '{' after scope"));
                    }
                    loop {
                        let field = match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::Identifier(f)) => f,
                            other => return Err(err(&format!("unexpected {:?} in scope", other))),
                        };
                        let rule = match self.next_token() {
                            Some(Token::Colon) => MirrorScope::Equals(self.try_json_literal().map_err(|e| err(&e))?),
                            Some(op @ (Token::CloseAngle | Token::OpenAngle)) => {
                                let bound = match (self.next_token(), self.next_token(), self.next_token()) {
                                    (Some(Token::Identifier(w)), Some(Token::Dot), Some(b)) if w == "window" => match b {
                                        Token::Keyword(Keyword::From) => true,
                                        Token::Identifier(b) if b == "from" => true,
                                        Token::Identifier(b) if b == "to" => false,
                                        _ => return Err(err("expected window.from or window.to")),
                                    },
                                    _ => return Err(err("expected window.from or window.to")),
                                };
                                if op == Token::CloseAngle { MirrorScope::After(bound) } else { MirrorScope::Before(bound) }
                            }
                            _ => return Err(err(&format!("scope: expected '{}: <value>' or '{} > window.from'", field, field))),
                        };
                        mirror.scope.push((field, rule));
                    }
                }
                Some(Token::Identifier(k)) if k == "map" => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        return Err(err("expected '{' after map"));
                    }
                    loop {
                        let field = match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::Identifier(f)) => f,
                            other => return Err(err(&format!("unexpected {:?} in map", other))),
                        };
                        if self.next_token() != Some(Token::Colon) {
                            return Err(err(&format!("expected ':' after '{}' in map", field)));
                        }
                        let pos = self.current;
                        let value = match self.next_token() {
                            Some(Token::Identifier(w)) if w == "item" => {
                                let mut path = Vec::new();
                                loop {
                                    let dot = self.current;
                                    match (self.next_token(), self.next_token()) {
                                        (Some(Token::Dot), Some(Token::Identifier(part))) => path.push(part),
                                        (Some(Token::Dot), Some(Token::Keyword(k))) => path.push(k.to_string()),
                                        _ => {
                                            self.reset(dot);
                                            break;
                                        }
                                    }
                                }
                                if path.is_empty() {
                                    return Err(err(&format!("map: '{}': expected item.<field>", field)));
                                }
                                MapValue::Path(path)
                            }
                            _ => {
                                self.reset(pos);
                                MapValue::Literal(
                                    self.try_json_literal()
                                        .map_err(|e| err(&format!("map: '{}': {} (or item.<field>)", field, e)))?,
                                )
                            }
                        };
                        mirror.map.push((field, value));
                    }
                }
                Some(Token::Identifier(k)) if k == "skip" => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        return Err(err("expected '{' after skip"));
                    }
                    loop {
                        match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::Identifier(w)) if w == "item" => {}
                            other => return Err(err(&format!("skip: expected item.<field>, got {:?}", other))),
                        }
                        let mut path = Vec::new();
                        loop {
                            let dot = self.current;
                            match (self.next_token(), self.next_token()) {
                                (Some(Token::Dot), Some(Token::Identifier(part))) => path.push(part),
                                (Some(Token::Dot), Some(Token::Keyword(k))) => path.push(k.to_string()),
                                _ => {
                                    self.reset(dot);
                                    break;
                                }
                            }
                        }
                        if path.is_empty() {
                            return Err(err("skip: expected item.<field>"));
                        }
                        let pos = self.current;
                        let operator = match self.next_token() {
                            Some(Token::Identifier(w)) if w == "matches" => None,
                            _ => {
                                self.reset(pos);
                                Some(self.try_filter_operator().ok_or_else(|| {
                                    err("skip: expected an operator (==, !=, <, >, in, not in, like, ilike, contains, matches)")
                                })?)
                            }
                        };
                        let value = self.try_json_literal().map_err(|e| err(&format!("skip: {}", e)))?;
                        mirror.skip.push(crate::parser::MirrorSkip { path, operator, value });
                    }
                }
                Some(Token::Identifier(k)) if k == "window" => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        return Err(err("expected '{' after window"));
                    }
                    loop {
                        let key = match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::Keyword(Keyword::From)) => "from".to_string(),
                            Some(Token::Identifier(k)) => k,
                            other => return Err(err(&format!("unexpected {:?} in window", other))),
                        };
                        if self.next_token() != Some(Token::Colon) {
                            return Err(err(&format!("expected ':' after '{}' in window", key)));
                        }
                        match (key.as_str(), self.next_token()) {
                            ("from", Some(Token::Identifier(v))) if ["now", "today", "week_start"].contains(&v.as_str()) => {
                                mirror.window_from = v;
                                let pos = self.current;
                                if self.next_token() == Some(Token::OpenParen) {
                                    match self.next_token() {
                                        Some(Token::CloseParen) => {}
                                        Some(Token::StringLiteral(zone)) if self.next_token() == Some(Token::CloseParen) => {
                                            mirror.window_zone = Some(zone)
                                        }
                                        _ => return Err(err("expected window from: week_start(\"<zone>\")")),
                                    }
                                } else {
                                    self.reset(pos);
                                }
                            }
                            ("days", Some(Token::Identifier(v))) => {
                                mirror.window_days =
                                    v.parse::<i64>().ok().filter(|d| *d > 0).ok_or_else(|| err("expected days: <positive number>"))?
                            }
                            _ => return Err(err("window: expected from: now | today | week_start and days: <n>")),
                        }
                    }
                }
                other => {
                    return Err(err(&format!(
                        "unexpected {:?} (expected source, into, key, scope, skip, map, window)",
                        other
                    )));
                }
            }
        }
        if mirror.source.is_empty() || mirror.into.is_empty() {
            return Err(err("must declare 'source' and 'into'"));
        }
        if mirror.key.is_empty() {
            return Err(err("must declare 'key: [...]'"));
        }
        Ok(Ast::Mirror(mirror))
    }
}
