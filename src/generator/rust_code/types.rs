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

#[derive(Debug, Clone)]
pub struct RustType {
    pub scope: Scope,
    pub name: String,
    pub templates: Vec<RustType>,
    pub fields: Vec<RustField>,
    pub include: Option<Include>,
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
        }
    }

    pub fn generic(name: &str, templates: Vec<RustType>, fields: Vec<RustField>) -> Self {
        RustType {
            scope: Scope::Public,
            name: name.to_string(),
            templates,
            fields,
            include: None,
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
}

impl RustCode for RustType {
    fn to_rust_code(&self) -> String {
        let fields: Vec<FieldView> = self
            .fields
            .iter()
            .map(|field| FieldView {
                name: field.name.clone(),
                field_type: field.field_type.to_call_string(Vec::new()),
            })
            .collect();

        let mut context = Context::new();
        context.insert("is_public", &(self.scope == Scope::Public));
        context.insert("name", &self.name);
        context.insert("fields", &fields);
        render("types.rs.tera", &context)
    }
}
