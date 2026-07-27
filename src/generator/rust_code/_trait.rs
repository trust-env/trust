use crate::generator::rust_code::{
    RustCode,
    include::Include,
    render::render,
    service::{Method, Service},
    types::RustType,
};
use tera::Context;

pub struct Trait {
    pub name: String,
    pub templates: Vec<RustType>,
    pub methods: Vec<Method>,
    pub implements: Vec<Trait>,
}

impl Trait {
    pub fn send() -> Self {
        Trait {
            name: "Send".to_string(),
            templates: vec![],
            methods: vec![],
            implements: vec![],
        }
    }

    pub fn sync() -> Self {
        Trait {
            name: "Sync".to_string(),
            templates: vec![],
            methods: vec![],
            implements: vec![],
        }
    }

    pub fn storage(e: RustType, error: &str) -> Self {
        Trait {
            name: format!("Storage<{}, {}>", e.to_call_string(vec![]), error),
            templates: vec![],
            methods: vec![],
            implements: vec![],
        }
    }
}

impl RustCode for Trait {
    fn to_rust_code(&self) -> String {
        let templates: Vec<String> = self.templates.iter().map(|t| t.to_call_string(vec![])).collect();
        let implements: Vec<String> = self.implements.iter().map(|t| t.name.clone()).collect();
        let methods: Vec<String> = self.methods.iter().map(|m| m.to_rust_code()).collect();

        let mut context = Context::new();
        context.insert("name", &self.name);
        context.insert("templates", &templates);
        context.insert("implements", &implements);
        context.insert("methods", &methods);
        render("_trait/trait.rs.tera", &context)
    }
}

#[derive(Debug, Clone)]
pub struct TraitImpl {
    pub trait_name: String,
    pub templates: Vec<(String, RustType)>,
    pub methods: Vec<Method>,
    pub include: Option<Include>,
}

impl TraitImpl {
    pub fn to_rust_code(&self, service: &Service) -> String {
        let template_names: Vec<String> = self.templates.iter().map(|(name, _)| name.clone()).collect();
        let where_clauses: Vec<String> = self
            .templates
            .iter()
            .map(|(name, rust_type)| format!("\n\t{}: {}", name, rust_type.to_call_string(service.templates.clone())))
            .collect();
        let methods: Vec<String> = self.methods.iter().map(|m| m.to_rust_code()).collect();

        let mut context = Context::new();
        context.insert("has_templates", &!self.templates.is_empty());
        context.insert("template_names", &template_names);
        context.insert("where_clauses", &where_clauses);
        context.insert("trait_name", &self.trait_name);
        context.insert("service_name", &service.name);
        context.insert("methods", &methods);
        render("_trait/trait_impl.rs.tera", &context)
    }
}
