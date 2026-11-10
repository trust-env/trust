use crate::{
    lexer::{Keyword, Token},
    parser::{
        AiSpec, Ast, BlobCall, CodeSpec, ErrorMapping, FilterMapping, FilterOperator, FilterSource,
        OrderBy, Parser, UseCaseCall, UseCaseService,
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
                    Some(Token::Colon) => {
                        match (self.next_token(), self.next_token(), self.next_token()) {
                            (
                                Some(Token::Identifier(t)),
                                Some(Token::Dot),
                                Some(Token::Identifier(m)),
                            ) => {
                                target = Some(t);
                                method = Some(m);
                            }
                            _ => {
                                self.reset(pos);
                                return Err(
                                    "Expected '<Target>.<method>' after 'name:'".to_string()
                                );
                            }
                        }
                    }
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

    /// A word in name position: identifiers and keywords alike (`status`,
    /// `input`, ... are valid field names).
    fn try_name(&mut self) -> Option<String> {
        match self.next_token() {
            Some(Token::Identifier(name)) => Some(name),
            Some(Token::Keyword(k)) => Some(k.to_string()),
            _ => {
                self.revert(1);
                None
            }
        }
    }

    /// The operator of a `filters {}` line, right after the field name.
    fn try_filter_operator(&mut self) -> Option<FilterOperator> {
        let pos = self.current;
        let op = match self.next_token() {
            Some(Token::Colon) => FilterOperator::Eq,
            Some(Token::Equal) => {
                let after = self.current;
                if self.next_token() != Some(Token::Equal) {
                    self.reset(after);
                }
                FilterOperator::Eq
            }
            Some(Token::Bang) if self.next_token() == Some(Token::Equal) => FilterOperator::NotEq,
            Some(Token::CloseAngle) => {
                let after = self.current;
                if self.next_token() == Some(Token::Equal) {
                    FilterOperator::Gte
                } else {
                    self.reset(after);
                    FilterOperator::Gt
                }
            }
            Some(Token::OpenAngle) => {
                let after = self.current;
                if self.next_token() == Some(Token::Equal) {
                    FilterOperator::Lte
                } else {
                    self.reset(after);
                    FilterOperator::Lt
                }
            }
            Some(Token::Identifier(word)) => match word.as_str() {
                "in" => FilterOperator::In,
                "not" if matches!(self.next_token(), Some(Token::Identifier(w)) if w == "in") => {
                    FilterOperator::NotIn
                }
                "like" => FilterOperator::Like,
                "ilike" => FilterOperator::ILike,
                "contains" => FilterOperator::Contains,
                _ => {
                    self.reset(pos);
                    return None;
                }
            },
            _ => {
                self.reset(pos);
                return None;
            }
        };
        Some(op)
    }

    /// Parses one `filters {}` line: `<field> <op> <source>`, where `<op>`
    /// is `:` / `=` / `==` (equality), `!=`, `>`, `>=`, `<`, `<=`, `in`,
    /// `not in`, `like`, `ilike` or `contains`, and `<source>` is
    /// `input.<field>`, a literal (quoted string or bare word such as
    /// `true` / `42`), a `[a, b]` list, `now()` or `today()`.
    fn try_filter_mapping(&mut self) -> Result<FilterMapping, String> {
        let pos = self.current;
        let fail = |parser: &mut Parser, message: String| {
            parser.reset(pos);
            Err(message)
        };
        let Some(field) = self.try_name() else {
            return fail(self, "Expected field identifier in 'filters' block".to_string());
        };
        let Some(operator) = self.try_filter_operator() else {
            return fail(
                self,
                format!("Expected an operator (:, =, !=, >, >=, <, <=, in, not in, like, ilike, contains) after '{}' in 'filters' block", field),
            );
        };
        let source = match self.next_token() {
            Some(Token::Keyword(Keyword::Input)) => match self.next_token() {
                Some(Token::Dot) => match self.try_name() {
                    Some(input_field) => FilterSource::Input(input_field),
                    None => return fail(self, format!("Expected a field name after 'input.' for '{}'", field)),
                },
                _ => return fail(self, format!("Expected 'input.<field>' for '{}'", field)),
            },
            Some(Token::OpenBracket) => {
                let mut values = Vec::new();
                loop {
                    match self.next_token() {
                        Some(Token::CloseBracket) => break,
                        Some(Token::Comma) => continue,
                        Some(Token::StringLiteral(v)) | Some(Token::Identifier(v)) => values.push(v),
                        _ => return fail(self, format!("Expected literal values in the list for '{}'", field)),
                    }
                }
                FilterSource::List(values)
            }
            Some(Token::Identifier(word)) if (word == "now" || word == "today") => {
                let after = self.current;
                match (self.next_token(), self.next_token()) {
                    (Some(Token::OpenParen), Some(Token::CloseParen)) => {
                        if word == "now" { FilterSource::Now } else { FilterSource::Today(None) }
                    }
                    (Some(Token::OpenParen), Some(Token::StringLiteral(tz)))
                        if word == "today" && self.next_token() == Some(Token::CloseParen) =>
                    {
                        FilterSource::Today(Some(tz))
                    }
                    _ => {
                        self.reset(after);
                        FilterSource::Literal(word)
                    }
                }
            }
            Some(Token::StringLiteral(v)) | Some(Token::Identifier(v)) => FilterSource::Literal(v),
            _ => {
                return fail(
                    self,
                    format!("Expected 'input.<field>', a literal, a [list], now() or today() for '{}' in 'filters' block", field),
                );
            }
        };
        if operator.is_set() && matches!(source, FilterSource::Literal(_) | FilterSource::Now | FilterSource::Today(_)) {
            return fail(self, format!("'{}' uses 'in'/'not in': expected a [list] or input.<field>", field));
        }
        if !operator.is_set() && matches!(source, FilterSource::List(_)) {
            return fail(self, format!("'{}': a [list] needs 'in' or 'not in'", field));
        }
        Ok(FilterMapping { field, operator, source })
    }

    /// Parses `order_by: field [asc|desc], field [asc|desc] ...` after the
    /// `order_by` word.
    fn try_order_by(&mut self) -> Result<Vec<OrderBy>, String> {
        if self.next_token() != Some(Token::Colon) {
            self.revert(1);
            return Err("Expected ':' after 'order_by'".to_string());
        }
        let mut entries = Vec::new();
        loop {
            let Some(field) = self.try_name() else {
                return Err("Expected a field name in 'order_by'".to_string());
            };
            let pos = self.current;
            let descending = match self.next_token() {
                Some(Token::Identifier(dir)) if dir == "desc" => true,
                Some(Token::Identifier(dir)) if dir == "asc" => false,
                _ => {
                    self.reset(pos);
                    false
                }
            };
            entries.push(OrderBy { field, descending });
            let pos = self.current;
            if self.next_token() != Some(Token::Comma) {
                self.reset(pos);
                break;
            }
        }
        Ok(entries)
    }

    /// Parses `code { file: "..." infra: X ... errors { A B } deps { name: "version" } }`.
    /// None of `code`/`file`/`deps` are reserved keywords (all common
    /// identifiers), so they're matched by name, like `auth`'s `target`.
    fn try_code_block(&mut self) -> Result<CodeSpec, String> {
        match (self.next_token(), self.next_token()) {
            (Some(Token::Identifier(k)), Some(Token::OpenBrace)) if k == "code" => {}
            _ => {
                self.revert(2);
                return Err("Expected 'code {' block".to_string());
            }
        }

        let pos = self.current;
        let mut spec = CodeSpec::default();
        let mut file = None;

        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(k)) if k == "file" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(path))) => file = Some(path),
                        _ => {
                            self.reset(pos);
                            return Err(
                                "Expected ': \"<path>\"' after 'file' in 'code' block".to_string()
                            );
                        }
                    }
                }
                Some(Token::Identifier(k)) if k == "include" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::StringLiteral(path))) => spec.includes.push(path),
                        _ => {
                            self.reset(pos);
                            return Err(
                                "Expected ': \"<path>\"' after 'include' in 'code' block".to_string()
                            );
                        }
                    }
                }
                Some(Token::Keyword(Keyword::Infra)) => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::Identifier(name))) => {
                            spec.infras.push(name)
                        }
                        _ => {
                            self.reset(pos);
                            return Err(
                                "Expected ': <Infra>' after 'infra' in 'code' block".to_string()
                            );
                        }
                    }
                }
                Some(Token::Keyword(Keyword::Errors)) => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        self.reset(pos);
                        return Err("Expected '{' after 'errors' in 'code' block".to_string());
                    }
                    loop {
                        match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            Some(Token::Identifier(variant)) => {
                                if spec.errors.contains(&variant) {
                                    self.reset(pos);
                                    return Err(format!(
                                        "'code' block declares error '{}' more than once",
                                        variant
                                    ));
                                }
                                spec.errors.push(variant)
                            }
                            _ => {
                                self.reset(pos);
                                return Err(
                                    "Expected error variant names in 'code' block's 'errors'"
                                        .to_string(),
                                );
                            }
                        }
                    }
                }
                Some(Token::Identifier(k)) if k == "deps" => {
                    if self.next_token() != Some(Token::OpenBrace) {
                        self.reset(pos);
                        return Err("Expected '{' after 'deps' in 'code' block".to_string());
                    }
                    loop {
                        match self.next_token() {
                            Some(Token::CloseBrace) => break,
                            Some(Token::Comma) => continue,
                            // `chrono-tz` isn't an identifier: quote it.
                            Some(Token::Identifier(name)) | Some(Token::StringLiteral(name)) => {
                                match (self.next_token(), self.next_token()) {
                                    (Some(Token::Colon), Some(Token::StringLiteral(version))) => {
                                        spec.deps.push((name, version))
                                    }
                                    _ => {
                                        self.reset(pos);
                                        return Err(format!(
                                            "Expected ': \"<version>\"' after dependency '{}'",
                                            name
                                        ));
                                    }
                                }
                            }
                            _ => {
                                self.reset(pos);
                                return Err("Expected '<crate>: \"<version>\"' entries in 'deps'"
                                    .to_string());
                            }
                        }
                    }
                }
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse 'code' block".to_string());
                }
            }
        }

        match file {
            Some(file) => {
                spec.file = file;
                Ok(spec)
            }
            None => {
                self.reset(pos);
                Err("'code' block must declare 'file'".to_string())
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
            (
                Some(Token::Identifier(target)),
                Some(Token::Dot),
                Some(Token::Identifier(operation)),
            ) => Ok(BlobCall { target, operation }),
            _ => {
                self.revert(3);
                Err("Expected '<Target>.<operation>' after 'blob:'".to_string())
            }
        }
    }

    fn try_use_case_body(&mut self, name: String) -> Result<UseCaseService, String> {
        let mut input = None;
        let mut output = None;
        let mut calls = Vec::new();
        let mut ai = None;
        let mut blob = None;
        let mut filters = Vec::new();
        let mut owner = None;
        let mut code = None;
        let mut order_by = Vec::new();
        let mut transaction = false;
        let mut notify = None;
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
                Some(Token::Identifier(k)) if k == "owner" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::Identifier(field))) => owner = Some(field),
                        _ => {
                            self.reset(pos);
                            return Err("Expected ': <field>' after 'owner'".to_string());
                        }
                    }
                }
                Some(Token::Identifier(k)) if k == "code" => {
                    self.revert(1);
                    code = Some(self.try_code_block()?);
                }
                Some(Token::Identifier(k)) if k == "order_by" => {
                    order_by = self.try_order_by()?;
                }
                Some(Token::Identifier(k)) if k == "notify" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::Identifier(v))) if v == "true" || v == "false" => {
                            notify = Some(v == "true");
                        }
                        _ => {
                            self.reset(pos);
                            return Err("Expected ': true' or ': false' after 'notify'".to_string());
                        }
                    }
                }
                Some(Token::Identifier(k)) if k == "transaction" => {
                    match (self.next_token(), self.next_token()) {
                        (Some(Token::Colon), Some(Token::Identifier(v))) if v == "true" || v == "false" => {
                            transaction = v == "true";
                        }
                        _ => {
                            self.reset(pos);
                            return Err("Expected ': true' or ': false' after 'transaction'".to_string());
                        }
                    }
                }
                other => {
                    self.reset(pos);
                    return Err(format!("Failed to parse UseCase body: unexpected {:?}", other));
                }
            }
        }
        Ok(UseCaseService {
            name,
            input,
            output,
            calls,
            ai,
            blob,
            filters,
            owner,
            code,
            order_by,
            transaction,
            notify,
        })
    }

    pub(super) fn try_use_case_service(&mut self) -> Result<Ast, String> {
        match (self.next_token(), self.next_token(), self.next_token()) {
            (
                Some(Token::Keyword(Keyword::UseCase)),
                Some(Token::Identifier(name)),
                Some(Token::OpenBrace),
            ) => {
                let use_case = self.try_use_case_body(name.clone())?;
                let UseCaseService {
                    calls,
                    ai,
                    blob,
                    filters,
                    owner,
                    code,
                    ..
                } = &use_case;
                let modes_used = !calls.is_empty() as u8
                    + ai.is_some() as u8
                    + blob.is_some() as u8
                    + code.is_some() as u8;
                if modes_used > 1 {
                    return Err(format!(
                        "UseCase '{}' cannot declare more than one of 'call', 'ai', 'blob', 'code' — they're mutually exclusive",
                        name
                    ));
                }
                if !filters.is_empty() && code.is_some() {
                    return Err(format!(
                        "UseCase '{}': 'filters' cannot be combined with 'code'",
                        name
                    ));
                }
                if !use_case.order_by.is_empty() && calls.is_empty() {
                    return Err(format!(
                        "UseCase '{}': 'order_by' is only valid with 'call'",
                        name
                    ));
                }
                if use_case.transaction && calls.is_empty() && code.is_none() {
                    return Err(format!(
                        "UseCase '{}': 'transaction' is only valid with 'call' or 'code'",
                        name
                    ));
                }
                if owner.is_some() && calls.is_empty() {
                    return Err(format!(
                        "UseCase '{}': 'owner' is only valid with 'call'",
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
                Ok(Ast::UseCase(use_case))
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
    use crate::{lexer::Lexer, parser::AllowedType};

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
            "UseCase 'Bad' cannot declare more than one of 'call', 'ai', 'blob', 'code' — they're mutually exclusive"
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
        assert_eq!(uc.filters[0].input_field(), Some("spot_id"));
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
        assert_eq!(uc.filters[1].input_field(), Some("kind"));
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
    fn filters_accept_quoted_and_bare_literals() {
        let uc = parse_use_case(
            r#"use_case ListPublic {
                call: StickerInfra.list
                filters {
                    visibility: "public"
                    featured: true
                }
            }"#,
        );
        assert_eq!(
            uc.filters[0].source,
            FilterSource::Literal("public".to_string())
        );
        assert_eq!(
            uc.filters[1].source,
            FilterSource::Literal("true".to_string())
        );
        assert_eq!(uc.filters[0].input_field(), None);
    }

    #[test]
    fn filters_accept_a_keyword_as_the_field_name() {
        let uc = parse_use_case(
            r#"use_case ListPaid {
                call: OrderInfra.list
                filters {
                    status: "paid"
                }
            }"#,
        );
        assert_eq!(uc.filters[0].field, "status");
    }

    #[test]
    fn filters_reject_a_dangling_input_keyword() {
        // `input` alone (no `.field`) is neither form — a clear parse error.
        let src = r#"use_case Bad {
            call: RatingInfra.list
            filters {
                spot_id: input
            }
        }"#;
        assert!(parser_for(src).try_use_case_service().is_err());
    }

    #[test]
    fn owner_parses_the_field_name() {
        let uc = parse_use_case(
            r#"use_case ListMyCart {
                call: CartInfra.list
                owner: user_id
            }"#,
        );
        assert_eq!(uc.owner.as_deref(), Some("user_id"));
    }

    #[test]
    fn owner_without_a_call_is_rejected() {
        let src = r#"use_case Bad {
            blob: MediaInfra.presigned_put_url
            owner: user_id
        }"#;
        let err = parser_for(src).try_use_case_service().unwrap_err();
        assert!(
            err.contains("'owner' is only valid with 'call'"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn code_block_parses_file_infras_errors_and_deps() {
        let uc = parse_use_case(
            r#"use_case Checkout {
                input: CheckoutRequest
                output: CheckoutResponse
                code {
                    file: "./logic/checkout.rs"
                    infra: CartInfra
                    infra: PaymentApi
                    errors { EmptyCart PaymentUnavailable }
                    deps { hmac: "0.12" sha2: "0.10" }
                }
            }"#,
        );
        let code = uc.code.expect("expected a code spec");
        assert_eq!(code.file, "./logic/checkout.rs");
        assert_eq!(code.infras, vec!["CartInfra", "PaymentApi"]);
        assert_eq!(code.errors, vec!["EmptyCart", "PaymentUnavailable"]);
        assert_eq!(
            code.deps,
            vec![
                ("hmac".to_string(), "0.12".to_string()),
                ("sha2".to_string(), "0.10".to_string())
            ]
        );
    }

    #[test]
    fn code_block_without_file_is_rejected() {
        let src = r#"use_case Bad {
            code {
                infra: CartInfra
            }
        }"#;
        assert!(parser_for(src).try_use_case_service().is_err());
    }

    #[test]
    fn code_block_with_a_duplicate_error_is_rejected() {
        let src = r#"use_case Bad {
            code {
                file: "./x.rs"
                errors { A A }
            }
        }"#;
        assert!(parser_for(src).try_use_case_service().is_err());
    }

    #[test]
    fn declaring_both_call_and_code_is_rejected() {
        let src = r#"use_case Bad {
            call: Storage.create
            code {
                file: "./x.rs"
            }
        }"#;
        let err = parser_for(src).try_use_case_service().unwrap_err();
        assert!(
            err.contains("mutually exclusive"),
            "unexpected error: {}",
            err
        );
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
