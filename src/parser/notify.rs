use crate::{
    lexer::{Keyword, Token},
    parser::{Ast, BotDeclaration, ChannelDeclaration, NotificationsDeclaration, Parser},
};

impl Parser {
    /// `<kw> <Name> {` — the declaration's name.
    fn declaration_head(&mut self, kw: &str) -> Result<String, String> {
        let start = self.current;
        match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(k)), Some(Token::Identifier(name)), Some(Token::OpenBrace)) if k == kw => Ok(name),
            _ => {
                self.reset(start);
                Err(format!("Expected '{} <Name> {{'", kw))
            }
        }
    }

    /// `name` or `[a, b]` after a `:`.
    fn names_value(&mut self) -> Result<Vec<String>, String> {
        match self.next_token() {
            Some(Token::Identifier(name)) => Ok(vec![name]),
            Some(Token::OpenBracket) => {
                let mut names = Vec::new();
                loop {
                    match self.next_token() {
                        Some(Token::CloseBracket) => return Ok(names),
                        Some(Token::Comma) => continue,
                        Some(Token::Identifier(name)) => names.push(name),
                        other => return Err(format!("expected a name, got {:?}", other)),
                    }
                }
            }
            other => Err(format!("expected a name or [names], got {:?}", other)),
        }
    }

    /// `{ key: Name ... }`.
    fn name_map(&mut self, what: &str) -> Result<Vec<(String, Vec<String>)>, String> {
        if self.next_token() != Some(Token::OpenBrace) {
            return Err(format!("expected '{{' after {}", what));
        }
        let mut entries = Vec::new();
        loop {
            let key = match self.next_token() {
                Some(Token::CloseBrace) => return Ok(entries),
                Some(Token::Comma) => continue,
                Some(Token::Identifier(k)) => k,
                Some(Token::Keyword(k)) => k.to_string(),
                other => return Err(format!("{}: unexpected {:?}", what, other)),
            };
            if self.next_token() != Some(Token::Colon) {
                return Err(format!("{}: expected ':' after '{}'", what, key));
            }
            let names = self.names_value().map_err(|e| format!("{}: '{}': {}", what, key, e))?;
            entries.push((key, names));
        }
    }

    pub(super) fn try_channel_declaration(&mut self) -> Result<Ast, String> {
        let name = self.declaration_head("channel")?;
        let kind = match self.next_token() {
            Some(Token::Identifier(k)) if k == "telegram" || k == "webhook" => k,
            other => return Err(format!("channel '{}': expected telegram {{...}} or webhook {{...}}, got {:?}", name, other)),
        };
        let settings = self.try_string_map(&format!("channel '{}' {}", name, kind))?;
        let known: &[&str] = if kind == "telegram" { &["token", "chat", "api_url"] } else { &["url", "format"] };
        if let Some((unknown, _)) = settings.iter().find(|(k, _)| !known.contains(&k.as_str())) {
            return Err(format!("channel '{}': unknown key '{}' (known: {})", name, unknown, known.join(", ")));
        }
        let required: &[&str] = if kind == "telegram" { &["token", "chat"] } else { &["url"] };
        for key in required {
            if !settings.iter().any(|(k, _)| k == key) {
                return Err(format!("channel '{}' must declare '{}'", name, key));
            }
        }
        if let Some((_, format)) = settings.iter().find(|(k, _)| k == "format")
            && !["json", "discord", "slack"].contains(&format.as_str())
        {
            return Err(format!("channel '{}': format must be json, discord or slack", name));
        }
        if self.next_token() != Some(Token::CloseBrace) {
            return Err(format!("channel '{}': expected '}}'", name));
        }
        Ok(Ast::Channel(ChannelDeclaration { name, kind, settings }))
    }

    pub(super) fn try_notifications_declaration(&mut self) -> Result<Ast, String> {
        let name = self.declaration_head("notifications")?;
        let err = |what: String| format!("notifications '{}': {}", name, what);
        let mut decl = NotificationsDeclaration { name: name.clone(), infra: None, default: Vec::new(), fallback: None, routing: Vec::new() };
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Infra)) => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => decl.infra = Some(v),
                    _ => return Err(err("expected infra: <Infra>".to_string())),
                },
                Some(Token::Identifier(k)) if k == "default" => {
                    if self.next_token() != Some(Token::Colon) {
                        return Err(err("expected default: <Channel>".to_string()));
                    }
                    decl.default = self.names_value().map_err(&err)?;
                }
                Some(Token::Identifier(k)) if k == "fallback" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => decl.fallback = Some(v),
                    _ => return Err(err("expected fallback: <Channel>".to_string())),
                },
                Some(Token::Identifier(k)) if k == "routing" => decl.routing = self.name_map("routing").map_err(&err)?,
                other => return Err(err(format!("unexpected {:?} (expected infra, default, fallback, routing)", other))),
            }
        }
        if decl.default.is_empty() {
            return Err(err("must declare 'default'".to_string()));
        }
        Ok(Ast::Notifications(decl))
    }

    pub(super) fn try_bot_declaration(&mut self) -> Result<Ast, String> {
        let name = self.declaration_head("bot")?;
        let err = |what: String| format!("bot '{}': {}", name, what);
        let mut bot = BotDeclaration {
            name: name.clone(),
            channel: String::new(),
            path: String::new(),
            secret: None,
            user: 0,
            commands: Vec::new(),
            on_reply: Vec::new(),
            default: None,
            reply: None,
        };
        let single = |entries: Vec<(String, Vec<String>)>, what: &str| -> Result<Vec<(String, String)>, String> {
            entries
                .into_iter()
                .map(|(k, v)| match v.as_slice() {
                    [one] => Ok((k, one.clone())),
                    _ => Err(format!("{}: '{}' takes one use case", what, k)),
                })
                .collect()
        };
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(k)) if k == "channel" || k == "default" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => {
                        if k == "channel" {
                            bot.channel = v
                        } else {
                            bot.default = Some(v)
                        }
                    }
                    _ => return Err(err(format!("expected {}: <Name>", k))),
                },
                Some(Token::Identifier(k)) if k == "path" || k == "secret" || k == "reply" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(v))) => match k.as_str() {
                            "path" => bot.path = v,
                            "secret" => bot.secret = Some(v),
                            _ => bot.reply = Some(v),
                        },
                        _ => return Err(err(format!("expected {}: \"...\"", k))),
                    }
                }
                Some(Token::Identifier(k)) if k == "user" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => {
                        bot.user = v.parse().map_err(|_| err("expected user: <id>".to_string()))?
                    }
                    _ => return Err(err("expected user: <id>".to_string())),
                },
                Some(Token::Identifier(k)) if k == "commands" => {
                    bot.commands = single(self.name_map("commands").map_err(&err)?, "commands").map_err(&err)?
                }
                Some(Token::Identifier(k)) if k == "on_reply" => {
                    bot.on_reply = single(self.name_map("on_reply").map_err(&err)?, "on_reply").map_err(&err)?
                }
                other => {
                    return Err(err(format!(
                        "unexpected {:?} (expected channel, path, secret, user, commands, on_reply, default, reply)",
                        other
                    )));
                }
            }
        }
        if bot.channel.is_empty() || bot.path.is_empty() {
            return Err(err("must declare 'channel' and 'path'".to_string()));
        }
        Ok(Ast::Bot(bot))
    }
}
