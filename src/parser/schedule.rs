use crate::{
    lexer::{Keyword, Token},
    parser::{Ast, Parser, ScheduleDeclaration, ScheduleJob},
};

impl Parser {
    /// `=>` (lexed as `=` then `>`) or `->`.
    fn try_fat_arrow(&mut self) -> bool {
        let pos = self.current;
        match self.next_token() {
            Some(Token::Arrow) => true,
            Some(Token::Equal) if self.next_token() == Some(Token::CloseAngle) => true,
            _ => {
                self.reset(pos);
                false
            }
        }
    }

    /// `tz: "Europe/Paris"`, after the `tz` word has been read.
    fn try_tz_value(&mut self) -> Result<String, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Colon), Some(Token::StringLiteral(tz))) => Ok(tz),
            _ => Err("Expected ': \"<IANA time zone>\"' after 'tz'".to_string()),
        }
    }

    /// Parses
    ///
    /// ```text
    /// schedule Jobs {
    ///     tz: "Europe/Paris"
    ///     user: 1
    ///     "*/15 * * * *" => SyncCalendarsUseCase
    ///     "30 7 * * *" => SendBriefUseCase tz: "Europe/Paris"
    /// }
    /// ```
    ///
    /// `schedule`, `tz` and `user` are contextual words, not keywords.
    pub(super) fn try_schedule_declaration(&mut self) -> Result<Ast, String> {
        let start = self.current;
        let name = match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(kw)), Some(Token::Identifier(name)), Some(Token::OpenBrace))
                if kw == "schedule" =>
            {
                name
            }
            _ => {
                self.reset(start);
                return Err("Expected 'schedule <Name> {'".to_string());
            }
        };

        let mut schedule = ScheduleDeclaration {
            name,
            tz: None,
            user: 0,
            jobs: Vec::new(),
        };
        let fail = |parser: &mut Parser, message: String| {
            parser.reset(start);
            Err(format!("schedule '{}': {}", schedule_name_hint(parser, start), message))
        };

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(word)) if word == "tz" => match self.try_tz_value() {
                    Ok(tz) => schedule.tz = Some(tz),
                    Err(e) => return fail(self, e),
                },
                Some(Token::Identifier(word)) if word == "user" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::Identifier(id))) => match id.parse::<i64>() {
                            Ok(id) => schedule.user = id,
                            Err(_) => return fail(self, format!("'user' must be an integer id, got '{}'", id)),
                        },
                        _ => return fail(self, "Expected ': <user id>' after 'user'".to_string()),
                    }
                }
                Some(Token::StringLiteral(cron)) => {
                    if !self.try_fat_arrow() {
                        return fail(self, format!("Expected '=>' after \"{}\"", cron));
                    }
                    let use_case = match self.next_token() {
                        Some(Token::Identifier(use_case)) => use_case,
                        _ => return fail(self, format!("Expected a use case name after \"{}\" =>", cron)),
                    };
                    let mut job = ScheduleJob { cron, use_case, tz: None, input: None };
                    let pos = self.current;
                    if self.next_token() == Some(Token::OpenBrace) {
                        self.revert(1);
                        match self.try_json_literal() {
                            Ok(value) => job.input = Some(value),
                            Err(e) => return fail(self, e),
                        }
                    } else {
                        self.reset(pos);
                    }
                    let pos = self.current;
                    match self.next_token() {
                        Some(Token::Identifier(word)) if word == "tz" => match self.try_tz_value() {
                            Ok(tz) => job.tz = Some(tz),
                            Err(e) => return fail(self, e),
                        },
                        _ => self.reset(pos),
                    }
                    schedule.jobs.push(job);
                }
                Some(Token::Keyword(k)) if k == Keyword::On => {
                    return fail(self, "'on' is not valid in a schedule".to_string());
                }
                other => {
                    return fail(
                        self,
                        format!(
                            "Expected '\"<cron>\" => <UseCase>', 'tz:' or 'user:', got {:?}",
                            other
                        ),
                    );
                }
            }
        }

        if schedule.jobs.is_empty() {
            self.reset(start);
            return Err(format!("schedule '{}' must declare at least one job", schedule.name));
        }
        Ok(Ast::Schedule(schedule))
    }
}

impl Parser {
    /// A JSON value written in Trust syntax: `{ key: value ... }` (commas
    /// optional), `[a, b]`, `"text"`, numbers, `true` / `false` / `null`.
    pub(super) fn try_json_literal(&mut self) -> Result<serde_json::Value, String> {
        match self.next_token() {
            Some(Token::OpenBrace) => {
                let mut map = serde_json::Map::new();
                loop {
                    let key = match self.next_token() {
                        Some(Token::CloseBrace) => break,
                        Some(Token::Comma) => continue,
                        Some(Token::Identifier(k)) | Some(Token::StringLiteral(k)) => k,
                        Some(Token::Keyword(k)) => k.to_string(),
                        other => return Err(format!("Expected a field name, got {:?}", other)),
                    };
                    if self.next_token() != Some(Token::Colon) {
                        return Err(format!("Expected ':' after '{}'", key));
                    }
                    let value = self.try_json_literal()?;
                    map.insert(key, value);
                }
                Ok(serde_json::Value::Object(map))
            }
            Some(Token::OpenBracket) => {
                let mut items = Vec::new();
                loop {
                    let pos = self.current;
                    match self.next_token() {
                        Some(Token::CloseBracket) => break,
                        Some(Token::Comma) => continue,
                        _ => {
                            self.reset(pos);
                            items.push(self.try_json_literal()?);
                        }
                    }
                }
                Ok(serde_json::Value::Array(items))
            }
            Some(Token::StringLiteral(text)) => Ok(serde_json::Value::String(text)),
            Some(Token::Identifier(word)) => match word.as_str() {
                "true" => Ok(serde_json::Value::Bool(true)),
                "false" => Ok(serde_json::Value::Bool(false)),
                "null" => Ok(serde_json::Value::Null),
                number => {
                    let integer = number
                        .parse::<i64>()
                        .map_err(|_| format!("'{}' is not a value (quote text: \"{}\")", number, number))?;
                    // `1.5` lexes as `1`, `.`, `5`.
                    let pos = self.current;
                    if let (Some(Token::Dot), Some(Token::Identifier(fraction))) = (self.next_token(), self.next_token()) {
                        if let Ok(float) = format!("{}.{}", integer, fraction).parse::<f64>() {
                            return Ok(serde_json::json!(float));
                        }
                    }
                    self.reset(pos);
                    Ok(serde_json::Value::from(integer))
                }
            },
            other => Err(format!("Expected a value, got {:?}", other)),
        }
    }
}

fn schedule_name_hint(parser: &Parser, start: usize) -> String {
    match parser.tokens.get(start + 1).map(|t| &t.value) {
        Some(Token::Identifier(name)) => name.clone(),
        _ => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(src: &str) -> Result<Ast, String> {
        let tokens = Lexer::new(&src.to_string()).run().unwrap();
        Parser::new(tokens).try_schedule_declaration()
    }

    #[test]
    fn parses_jobs_time_zones_and_user() {
        let ast = parse(
            r#"schedule Jobs {
                tz: "Europe/Paris"
                user: 1
                "*/15 * * * *" => SyncUseCase
                "30 7 * * *" -> BriefUseCase tz: "UTC"
            }"#,
        )
        .unwrap();
        let Ast::Schedule(s) = ast else { panic!("expected a schedule") };
        assert_eq!(s.name, "Jobs");
        assert_eq!(s.tz.as_deref(), Some("Europe/Paris"));
        assert_eq!(s.user, 1);
        assert_eq!(s.jobs.len(), 2);
        assert_eq!(s.jobs[0].cron, "*/15 * * * *");
        assert_eq!(s.jobs[0].use_case, "SyncUseCase");
        assert_eq!(s.jobs[0].tz, None);
        assert_eq!(s.jobs[1].tz.as_deref(), Some("UTC"));
    }

    #[test]
    fn missing_arrow_is_a_clear_error() {
        let err = parse(r#"schedule Jobs { "* * * * *" SyncUseCase }"#).unwrap_err();
        assert!(err.contains("Expected '=>'"), "{err}");
    }

    #[test]
    fn empty_schedule_is_rejected() {
        let err = parse("schedule Jobs { }").unwrap_err();
        assert!(err.contains("at least one job"), "{err}");
    }
}
