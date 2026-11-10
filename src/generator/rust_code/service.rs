

use crate::generator::rust_code::{
    _trait::TraitImpl,
    RustCode,
    render::render,
    types::{RustType, Scope},
};
use serde::Serialize;
use tera::Context;

#[derive(Debug, Clone)]
pub struct Method {
    pub name: String,
    pub is_pub: bool,
    pub is_async: bool,
    pub templates: Vec<RustType>,
    pub parameters: Vec<(String, RustType)>,
    pub return_type: RustType,
    pub content: String,
}

#[derive(Serialize)]
struct ParameterView {
    name: String,
    param_type: String,
}

impl RustCode for Method {
    fn to_rust_code(&self) -> String {
        let templates: Vec<String> = self
            .templates
            .iter()
            .map(|t| t.to_call_string(self.templates.clone()))
            .collect();
        let parameters: Vec<ParameterView> = self
            .parameters
            .iter()
            .map(|(name, param_type)| ParameterView {
                name: name.clone(),
                param_type: param_type.to_call_string(self.templates.clone()),
            })
            .collect();

        let mut context = Context::new();
        context.insert("is_pub", &self.is_pub);
        context.insert("is_async", &self.is_async);
        context.insert("name", &self.name);
        context.insert("templates", &templates);
        context.insert("parameters", &parameters);
        context.insert("return_type", &self.return_type.to_call_string(self.templates.clone()));
        context.insert("content", &self.content);
        render("service/method.rs.tera", &context)
    }
}

pub struct Attribute {
    pub scope: Scope,
    pub name: String,
    pub parameters: RustType,
}

impl Attribute {
    pub fn sql_connector() -> Self {
        Attribute {
            scope: Scope::Private,
            name: "db".to_string(),
            parameters: RustType::primitive("SQLConnector"),
        }
    }
}

pub struct Service {
    pub name: String,
    pub templates: Vec<RustType>,
    pub attrs: Vec<Attribute>,
    pub traits_impls: Vec<TraitImpl>,
}

impl Service {
    pub fn new(name: String) -> Self {
        Service {
            name,
            templates: Vec::new(),
            traits_impls: Vec::new(),
            attrs: Vec::new(),
        }
    }
}

#[derive(Serialize)]
struct AttrView {
    scope_str: &'static str,
    name: String,
    param_type: String,
}

impl RustCode for Service {
    fn to_rust_code(&self) -> String {
        let templates: Vec<String> = self
            .templates
            .iter()
            .map(|t| t.to_call_string(self.templates.clone()))
            .collect();
        let attrs: Vec<AttrView> = self
            .attrs
            .iter()
            .map(|attr| AttrView {
                scope_str: match attr.scope {
                    Scope::Public => "pub ",
                    Scope::Private => "",
                },
                name: attr.name.clone(),
                param_type: attr.parameters.to_call_string(self.templates.clone()),
            })
            .collect();
        let trait_impls: Vec<String> = self.traits_impls.iter().map(|ti| ti.to_rust_code(self)).collect();

        let mut context = Context::new();
        context.insert("name", &self.name);
        context.insert("templates", &templates);
        context.insert("attrs", &attrs);
        context.insert("trait_impls", &trait_impls);
        render("service/service.rs.tera", &context)
    }
}
