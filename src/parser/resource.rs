use crate::{
    lexer::{Keyword, Token},
    parser::{
        Ast, DatabaseDeclaration, DerivedDeclaration, DerivedType, JournalDeclaration, Parser, ResourceDeclaration,
        ResourceOperation, TypeDeclaration,
    },
};

const OPERATIONS: &[&str] = &["list", "get", "create", "update", "patch", "upsert", "save", "delete"];

impl Parser {
    /// `journal Actions { infra: ActionInfra owner: user_id undo: "/actions/:id/toggle" conflict: refuse }`.
    pub(super) fn try_journal_declaration(&mut self) -> Result<Ast, String> {
        let start = self.current;
        let name = match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(kw)), Some(Token::Identifier(name)), Some(Token::OpenBrace)) if kw == "journal" => name,
            _ => {
                self.reset(start);
                return Err("Expected 'journal <Name> {'".to_string());
            }
        };
        let (mut infra, mut owner, mut undo_path, mut overwrite) = (None, None, None, false);
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Infra)) => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => infra = Some(v),
                    _ => return Err(format!("journal '{}': expected infra: <Infra>", name)),
                },
                Some(Token::Identifier(k)) if k == "owner" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => owner = Some(v),
                    _ => return Err(format!("journal '{}': expected owner: <field>", name)),
                },
                Some(Token::Identifier(k)) if k == "undo" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::StringLiteral(v))) => undo_path = Some(v),
                    _ => return Err(format!("journal '{}': expected undo: \"/path/:id\"", name)),
                },
                Some(Token::Identifier(k)) if k == "conflict" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) if v == "refuse" || v == "overwrite" => overwrite = v == "overwrite",
                    _ => return Err(format!("journal '{}': expected conflict: refuse | overwrite", name)),
                },
                other => return Err(format!("journal '{}': unexpected {:?} (expected infra, owner, undo, conflict)", name, other)),
            }
        }
        Ok(Ast::Journal(JournalDeclaration {
            infra: infra.ok_or(format!("journal '{}' must declare 'infra'", name))?,
            owner: owner.ok_or(format!("journal '{}' must declare 'owner'", name))?,
            name,
            undo_path,
            overwrite,
        }))
    }

    /// `infra_set DayData { TaskInfra EventInfra }`.
    pub(super) fn try_infra_set(&mut self) -> Result<Ast, String> {
        let start = self.current;
        let name = match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(kw)), Some(Token::Identifier(name)), Some(Token::OpenBrace)) if kw == "infra_set" => name,
            _ => {
                self.reset(start);
                return Err("Expected 'infra_set <Name> {'".to_string());
            }
        };
        let mut members = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(member)) => members.push(member),
                other => return Err(format!("infra_set '{}': expected infra names, got {:?}", name, other)),
            }
        }
        Ok(Ast::InfraSet(name, members))
    }

    /// `database Main { url: "${DATABASE_URL}" pool: 16 }`.
    pub(super) fn try_database_declaration(&mut self) -> Result<Ast, String> {
        let start = self.current;
        let name = match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(kw)), Some(Token::Identifier(name)), Some(Token::OpenBrace)) if kw == "database" => name,
            _ => {
                self.reset(start);
                return Err("Expected 'database <Name> {'".to_string());
            }
        };
        let mut url = None;
        let mut pool = None;
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Identifier(k)) if k == "url" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::StringLiteral(v))) => url = Some(v),
                    _ => return Err(format!("database '{}': expected url: \"...\"", name)),
                },
                Some(Token::Identifier(k)) if k == "pool" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) if v.parse::<u32>().is_ok_and(|n| n > 0) => {
                        pool = v.parse().ok()
                    }
                    _ => return Err(format!("database '{}': expected pool: <positive number>", name)),
                },
                other => return Err(format!("database '{}': unexpected {:?} (expected url, pool)", name, other)),
            }
        }
        let url = url.ok_or(format!("database '{}' must declare 'url'", name))?;
        Ok(Ast::Database(DatabaseDeclaration { name, url, pool }))
    }

    /// `resource Tasks { infra: TaskInfra path: "/tasks" owner: user_id list { ... } create patch delete }`.
    pub(super) fn try_resource_declaration(&mut self) -> Result<Ast, String> {
        let start = self.current;
        let name = match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(kw)), Some(Token::Identifier(name)), Some(Token::OpenBrace)) if kw == "resource" => name,
            _ => {
                self.reset(start);
                return Err("Expected 'resource <Name> {'".to_string());
            }
        };
        let mut infra = None;
        let mut path = None;
        let mut owner = None;
        let mut singular = None;
        let mut journal = None;
        let mut singleton = false;
        let mut defaults = Vec::new();
        let mut operations: Vec<ResourceOperation> = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::CloseBrace) => break,
                Some(Token::Comma) => continue,
                Some(Token::Keyword(Keyword::Infra)) => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => infra = Some(v),
                    _ => return Err(format!("resource '{}': expected infra: <Infra>", name)),
                },
                Some(Token::Identifier(k)) if k == "path" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::StringLiteral(v))) => path = Some(v),
                    _ => return Err(format!("resource '{}': expected path: \"/...\"", name)),
                },
                Some(Token::Identifier(k)) if k == "singleton" => singleton = true,
                Some(Token::Identifier(k)) if k == "default" => {
                    self.revert(0);
                    match self.try_json_literal()? {
                        serde_json::Value::Object(map) => defaults.extend(map.into_iter()),
                        _ => return Err(format!("resource '{}': default expects {{ field: value }}", name)),
                    }
                }
                Some(Token::Identifier(k)) if k == "journal" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => journal = Some(v),
                    _ => return Err(format!("resource '{}': expected journal: <Journal>", name)),
                },
                Some(Token::Identifier(k)) if k == "name" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => singular = Some(v),
                    _ => return Err(format!("resource '{}': expected name: <Singular>", name)),
                },
                Some(Token::Identifier(k)) if k == "owner" => match (self.next_token(), self.next_token()) {
                    (Some(Token::Colon), Some(Token::Identifier(v))) => owner = Some(v),
                    _ => return Err(format!("resource '{}': expected owner: <field>", name)),
                },
                Some(Token::Identifier(op)) if OPERATIONS.contains(&op.as_str()) => {
                    if operations.iter().any(|o| o.kind == op) {
                        return Err(format!("resource '{}' declares '{}' twice", name, op));
                    }
                    let mut operation = ResourceOperation { kind: op.clone(), filters: Vec::new(), order_by: Vec::new() };
                    let pos = self.current;
                    if self.next_token() == Some(Token::OpenBrace) {
                        loop {
                            match self.next_token() {
                                Some(Token::CloseBrace) => break,
                                Some(Token::Comma) => continue,
                                Some(Token::Keyword(Keyword::Filters)) if op == "list" => {
                                    self.revert(1);
                                    operation.filters = self.try_filters_block()?;
                                }
                                Some(Token::Identifier(k)) if k == "order_by" && op == "list" => {
                                    operation.order_by = self.try_order_by()?;
                                }
                                other => {
                                    return Err(format!(
                                        "resource '{}': unexpected {:?} in '{}' (only list takes filters and order_by)",
                                        name, other, op
                                    ));
                                }
                            }
                        }
                    } else {
                        self.reset(pos);
                    }
                    operations.push(operation);
                }
                other => {
                    return Err(format!(
                        "resource '{}': unexpected {:?} (expected infra, path, owner, or one of {})",
                        name,
                        other,
                        OPERATIONS.join(", ")
                    ));
                }
            }
        }
        let infra = infra.ok_or(format!("resource '{}' must declare 'infra'", name))?;
        let path = path.ok_or(format!("resource '{}' must declare 'path'", name))?;
        if operations.is_empty() {
            return Err(format!("resource '{}' must declare at least one operation ({})", name, OPERATIONS.join(", ")));
        }
        if singleton && owner.is_none() {
            return Err(format!("resource '{}': 'singleton' needs 'owner'", name));
        }
        if singleton && operations.iter().any(|o| !["get", "upsert"].contains(&o.kind.as_str())) {
            return Err(format!("resource '{}': a singleton only has 'get' and 'upsert'", name));
        }
        Ok(Ast::Resource(ResourceDeclaration { name, infra, path, owner, singular, journal, singleton, defaults, operations }))
    }

    /// After `type Name =`: `Omit<...>` / `Pick<...>` / `Partial<...>`.
    fn try_derived_type(&mut self) -> Result<DerivedType, String> {
        let pos = self.current;
        match self.next_token() {
            Some(Token::Identifier(kind)) if kind == "Omit" || kind == "Pick" || kind == "Partial" => {
                if self.next_token() != Some(Token::OpenAngle) {
                    self.reset(pos);
                    return Err(format!("Expected '<' after '{}'", kind));
                }
                let base = self.try_derived_type()?;
                let mut fields = Vec::new();
                loop {
                    match self.next_token() {
                        Some(Token::CloseAngle) => break,
                        Some(Token::Comma) => continue,
                        Some(Token::Identifier(f)) if kind != "Partial" => fields.push(f),
                        Some(Token::Keyword(k)) if kind != "Partial" => fields.push(k.to_string()),
                        other => {
                            self.reset(pos);
                            return Err(format!("Unexpected {:?} in {}<...>", other, kind));
                        }
                    }
                }
                Ok(match kind.as_str() {
                    "Omit" => DerivedType::Omit(Box::new(base), fields),
                    "Pick" => DerivedType::Pick(Box::new(base), fields),
                    _ => DerivedType::Partial(Box::new(base)),
                })
            }
            Some(Token::Identifier(name)) => Ok(DerivedType::Named(name)),
            other => {
                self.reset(pos);
                Err(format!("Expected a type name, Omit, Pick or Partial, got {:?}", other))
            }
        }
    }

    /// `type Name = Omit<Base, a> [+ { extra: Field }]` — `None` when the
    /// right-hand side isn't a derived type (a plain alias then).
    pub(super) fn try_derived_declaration(&mut self, name: &str) -> Result<Option<Ast>, String> {
        let pos = self.current;
        match (self.tokens.get(self.current).map(|t| &t.value), self.tokens.get(self.current + 1).map(|t| &t.value)) {
            (Some(Token::Identifier(kind)), _) if kind == "Omit" || kind == "Pick" || kind == "Partial" => {}
            // `type X = Task + { ... }`.
            (Some(Token::Identifier(_)), Some(Token::Plus)) => {}
            _ => return Ok(None),
        }
        let base = self.try_derived_type()?;
        let mut extra = Vec::new();
        let after = self.current;
        if self.next_token() == Some(Token::Plus) {
            if self.next_token() != Some(Token::OpenBrace) {
                self.reset(pos);
                return Err("Expected '{' after '+'".to_string());
            }
            // Consumes the closing `}` too.
            let (fields, _) = self.try_type_decl_body()?;
            extra = fields;
        } else {
            self.reset(after);
        }
        Ok(Some(Ast::TypeDecl(TypeDeclaration::Derived(DerivedDeclaration {
            name: name.to_string(),
            base,
            extra,
            export: None,
        }))))
    }
}
