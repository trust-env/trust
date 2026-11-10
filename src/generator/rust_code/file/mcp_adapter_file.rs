use std::path::PathBuf;

use heck::ToSnakeCase;
use serde::Serialize;
use tera::Context;

use crate::generator::rust_code::{
    env_template,
    fragments::{shared_bound, where_clause},
    imports::crate_use,
    render::render,
};

/// One tool of an `mcp {}` adapter, resolved against its use case.
pub struct McpToolBinding {
    pub name: String,
    pub description: String,
    pub use_case_name: String,
    pub input_type_name: String,
    /// JSON Schema of the use case input (serialized).
    pub schema: String,
}

/// How MCP requests over HTTP are authenticated.
pub enum McpAuth {
    None,
    Jwt,
    Secret {
        value_expr: String,
        /// Lowercase header; `None` means `Authorization: Bearer`.
        header: Option<String>,
        user: i64,
    },
}

/// One `resources {}` entry, resolved against its use case.
pub struct McpResourceBinding {
    pub name: String,
    pub uri: String,
    pub description: String,
    pub use_case_name: String,
    pub input_type_name: String,
}

/// `oauth {}` settings, as Rust expressions.
pub struct McpOAuthBinding {
    pub public_url_expr: String,
    pub password_expr: String,
    pub user: i64,
}

pub struct McpAdapterFile {
    pub name: String,
    pub tools: Vec<McpToolBinding>,
    pub resources: Vec<McpResourceBinding>,
    pub auth: McpAuth,
    pub oauth: Option<McpOAuthBinding>,
    pub instructions: Option<String>,
}

#[derive(Serialize)]
struct UseCaseView {
    field: String,
    type_param: String,
}

#[derive(Serialize)]
struct ToolView {
    name_literal: String,
    description_literal: String,
    schema_literal: String,
    input: String,
    field: String,
}

impl McpAdapterFile {
    /// Distinct use cases, in first-tool order, each with its generic param.
    fn use_cases(&self) -> Vec<(String, String)> {
        let mut seen: Vec<(String, String)> = Vec::new();
        let names = self
            .tools
            .iter()
            .map(|t| &t.use_case_name)
            .chain(self.resources.iter().map(|r| &r.use_case_name));
        for name in names {
            if !seen.iter().any(|(uc, _)| uc == name) {
                let param = format!("C{}", seen.len());
                seen.push((name.clone(), param));
            }
        }
        seen
    }

    /// Use case names in constructor order (for `main.rs`).
    pub fn constructor_use_cases(&self) -> Vec<String> {
        self.use_cases().into_iter().map(|(uc, _)| uc).collect()
    }

    pub fn write(&self, folder: &PathBuf) -> Result<(), String> {
        let use_cases = self.use_cases();
        let bounds: Vec<(String, Vec<String>)> = use_cases
            .iter()
            .map(|(uc, param)| (param.clone(), shared_bound(&format!("{}Trait", uc))))
            .collect();
        let generics: Vec<&str> = use_cases.iter().map(|(_, p)| p.as_str()).collect();

        let mut crate_items: Vec<String> = Vec::new();
        for (uc, _) in &use_cases {
            crate_items.push(format!("{}Trait", uc));
        }
        for input in self.tools.iter().map(|t| &t.input_type_name).chain(self.resources.iter().map(|r| &r.input_type_name)) {
            if !crate_items.contains(input) {
                crate_items.push(input.clone());
            }
        }
        #[derive(Serialize)]
        struct ResourceView {
            uri_literal: String,
            name_literal: String,
            description_literal: String,
            input: String,
            field: String,
        }
        let resources: Vec<ResourceView> = self
            .resources
            .iter()
            .map(|r| ResourceView {
                uri_literal: format!("{:?}", r.uri),
                name_literal: format!("{:?}", r.name),
                description_literal: format!("{:?}", r.description),
                input: r.input_type_name.clone(),
                field: r.use_case_name.to_snake_case(),
            })
            .collect();

        let tools: Vec<ToolView> = self
            .tools
            .iter()
            .map(|t| ToolView {
                name_literal: format!("{:?}", t.name),
                description_literal: format!("{:?}", t.description),
                schema_literal: format!("{:?}", t.schema),
                input: t.input_type_name.clone(),
                field: t.use_case_name.to_snake_case(),
            })
            .collect();

        let mut context = Context::new();
        context.insert("name", &self.name);
        context.insert("server_name_literal", &format!("{:?}", self.name));
        context.insert("generics", &generics.join(", "));
        context.insert("where_clause", &where_clause(&bounds));
        context.insert(
            "use_cases",
            &use_cases
                .iter()
                .map(|(uc, p)| UseCaseView { field: uc.to_snake_case(), type_param: p.clone() })
                .collect::<Vec<_>>(),
        );
        context.insert("tools", &tools);
        context.insert("resources", &resources);
        context.insert("oauth", &self.oauth.is_some());
        if let Some(oauth) = &self.oauth {
            context.insert("public_url_expr", &oauth.public_url_expr);
            context.insert("password_expr", &oauth.password_expr);
            context.insert("oauth_user", &oauth.user);
        }
        context.insert("crate_use", &crate_use(&crate_items));
        context.insert("instructions_literal", &self.instructions.as_ref().map(|i| format!("{:?}", i)));
        match &self.auth {
            McpAuth::None => context.insert("auth", "none"),
            McpAuth::Jwt => context.insert("auth", "jwt"),
            McpAuth::Secret { value_expr, header, user } => {
                context.insert("auth", "secret");
                context.insert("secret_expr", value_expr);
                context.insert("secret_header_literal", &format!("{:?}", header.as_deref().unwrap_or("authorization")));
                context.insert("secret_bearer", &header.is_none());
                context.insert("secret_user", user);
            }
        }
        let code = render("file/mcp_adapter/adapter.rs.tera", &context);
        let path = folder.join(format!("{}.rs", self.name.to_snake_case()));
        std::fs::write(&path, code).map_err(|e| format!("Failed to write MCP adapter {:?}: {}", path, e))
    }

    pub fn secret_expr(template: &str) -> String {
        env_template::to_rust_expr_lenient(template)
    }
}
