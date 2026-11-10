use crate::generator::rust_code::{RustCode, include::Include, render::render};
use heck::ToSnakeCase;
use serde::Serialize;
use tera::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Public,
    Private,
}

#[derive(Debug, Clone)]
pub struct RustField {
    pub name: String,
    pub field_type: RustType,
}

/// What a resolved type is beyond its Rust spelling — drives the serde
/// attributes, column type and `sqlx` binding generated for it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TypeKind {
    #[default]
    Plain,
    /// A declared `enum`: its wire values, in declaration order.
    Enum(Vec<String>),
    /// A `Regex<...>` string: the resolved pattern.
    Regex(String),
    /// A declared struct (`type X { ... }`).
    Struct,
}

#[derive(Debug, Clone)]
pub struct RustType {
    pub scope: Scope,
    pub name: String,
    pub templates: Vec<RustType>,
    pub fields: Vec<RustField>,
    pub include: Option<Include>,
    pub kind: TypeKind,
}

impl RustType {
    pub fn to_file_name(&self) -> String {
        self.name.to_snake_case()
    }

    pub fn primitive(name: &str) -> Self {
        RustType {
            scope: Scope::Public,
            name: name.to_string(),
            templates: Vec::new(),
            fields: Vec::new(),
            include: None,
            kind: TypeKind::Plain,
        }
    }

    pub fn generic(name: &str, templates: Vec<RustType>, fields: Vec<RustField>) -> Self {
        RustType {
            scope: Scope::Public,
            name: name.to_string(),
            templates,
            fields,
            include: None,
            kind: TypeKind::Plain,
        }
    }

    pub fn with_kind(mut self, kind: TypeKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn is_option(&self) -> bool {
        self.name == "Option"
    }

    /// `Option<T>` -> `T`, anything else unchanged.
    pub fn unwrap_option(&self) -> &RustType {
        if self.is_option() {
            self.templates.first().unwrap_or(self)
        } else {
            self
        }
    }

    pub fn is_enum(&self) -> bool {
        matches!(self.kind, TypeKind::Enum(_))
    }

    /// Stored as a `JSONB` column and bound through `sqlx::types::Json`:
    /// lists and declared structs (`Json` itself binds natively).
    pub fn is_json_wrapped(&self) -> bool {
        let t = self.unwrap_option();
        t.name == "Vec" || t.kind == TypeKind::Struct
    }

    /// The `Regex<...>` pattern of this field (or of its `Option` inner).
    pub fn regex_pattern(&self) -> Option<&str> {
        match &self.unwrap_option().kind {
            TypeKind::Regex(p) => Some(p),
            _ => None,
        }
    }

    pub fn add_field(&mut self, field: RustField) {
        self.fields.push(field);
    }

    pub fn to_call_string(&self, templates: Vec<RustType>) -> String {
        if self.templates.is_empty() {
            self.name.clone()
        } else {
            let template_names: Vec<String> = self
                .templates
                .iter()
                .map(|t| t.to_call_string(templates.clone()))
                .collect();
            format!("{}<{}>", self.name, template_names.join(", "))
        }
    }
}

#[derive(Serialize)]
struct FieldView {
    name: String,
    field_type: String,
    regex_fn: Option<String>,
    regex_optional: bool,
    sqlx_json: &'static str,
}

impl RustCode for RustType {
    fn to_rust_code(&self) -> String {
        let fields: Vec<FieldView> = self
            .fields
            .iter()
            .map(|field| FieldView {
                name: field.name.clone(),
                field_type: field.field_type.to_call_string(Vec::new()),
                regex_fn: None,
                regex_optional: false,
                sqlx_json: "",
            })
            .collect();

        let mut context = Context::new();
        context.insert("is_public", &(self.scope == Scope::Public));
        context.insert("name", &self.name);
        context.insert("fields", &fields);
        render("types.rs.tera", &context)
    }
}
