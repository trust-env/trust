use std::{fs::File, io::Write, path::PathBuf};

use heck::ToSnakeCase;

use crate::generator::rust_code::{
    fragments::{indented_body, type_params, where_block},
    imports::{LocalImports, common_use},
    render::render,
    types::RustType,
};
use tera::Context;

/// The infra an `ai {}` use case is allowed to call, if any — unlike
/// `ResolvedCall`, there's no method/call_kind: the AI decides which
/// storage methods to call and how.
pub struct AiInfraBinding {
    pub infra_name: String,
    pub entity_type: RustType,
    pub storage_trait: String,
    pub storage_error: String,
}

/// Generates a use case whose `execute()` body is AI-authored text instead
/// of a deterministic CRUD template. Input/output are used directly (no
/// synthetic `{Name}Input`/`{Name}Output` wrapper) since there's no
/// entity-driven guessing to support here.
pub struct AiUseCaseFile {
    name: String,
    input: RustType,
    output: RustType,
    infra: Option<AiInfraBinding>,
    body: String,
    file: File,
}

impl AiUseCaseFile {
    pub fn new(
        name: String,
        input: RustType,
        output: RustType,
        infra: Option<AiInfraBinding>,
        body: String,
        folder: PathBuf,
    ) -> Result<Self, String> {
        let file_path = folder.join(format!("{}.rs", name.to_snake_case()));

        Ok(AiUseCaseFile {
            file: File::create(&file_path)
                .map_err(|e| format!("Failed to create use case file: {}", e))?,
            name,
            input,
            output,
            infra,
            body,
        })
    }

    fn error_name(&self) -> String {
        format!("{}Error", self.name)
    }

    fn trait_name(&self) -> String {
        format!("{}Trait", self.name)
    }

    /// Whether the AI-authored body references `name` as a whole word — the
    /// entity type is only ever used from inside that body (never the
    /// static scaffold around it), so importing it unconditionally would
    /// warn `unused_imports` whenever the body happens not to name it
    /// directly (e.g. it only binds a variable via `Option<_>` inference).
    fn body_references(&self, name: &str) -> bool {
        self.body
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == name)
    }

    fn gen_imports(&self) -> String {
        let mut imports = LocalImports::from_fields(&self.input.fields, "");
        for field in &self.output.fields {
            imports.add(&field.field_type, "");
        }
        if let Some(infra) = &self.infra {
            if self.body_references(&infra.entity_type.name) {
                imports.add(&infra.entity_type, "");
            }
            if !imports.types.contains(&infra.storage_trait) {
                imports.types.push(infra.storage_trait.clone());
            }
            if !imports.types.contains(&infra.storage_error) {
                imports.types.push(infra.storage_error.clone());
            }
        }
        if !imports.types.contains(&self.input.name) {
            imports.types.push(self.input.name.clone());
        }
        if !imports.types.contains(&self.output.name) {
            imports.types.push(self.output.name.clone());
        }

        let mut code = common_use(&["UseCase"]);
        code.push_str(&imports.to_use_statements());
        code
    }

    /// Fixed at exactly `NotFound`/`StorageError(String)` — no `errors {}`
    /// customization for `ai {}` use cases in this iteration, so the
    /// adapter's generated status-mapping match stays exhaustive without
    /// needing to know about AI-specific error targets.
    fn gen_error(&self) -> String {
        let name = self.error_name();

        let mut context = Context::new();
        context.insert("name", &name);
        context.insert("storage_error", &self.infra.as_ref().map(|infra| infra.storage_error.clone()));
        render("file/ai_use_case/error.rs.tera", &context)
    }

    fn gen_trait(&self) -> String {
        let mut context = Context::new();
        context.insert("trait_name", &self.trait_name());
        context.insert("input", &self.input.name);
        context.insert("output", &self.output.name);
        context.insert("error", &self.error_name());
        render("file/ai_use_case/trait.rs.tera", &context)
    }

    fn gen_struct(&self) -> String {
        let mut context = Context::new();
        context.insert("name", &self.name);
        match &self.infra {
            Some(infra) => {
                context.insert("has_infra", &true);
                context.insert("storage_trait", &infra.storage_trait);
                context.insert("field", &infra.infra_name.to_snake_case());
            }
            None => {
                context.insert("has_infra", &false);
            }
        }
        render("file/ai_use_case/struct.rs.tera", &context)
    }

    fn indent_body(&self) -> String {
        let lines: Vec<&str> = self.body.lines().collect();
        indented_body(&lines)
    }

    fn gen_impl(&self) -> String {
        let (generics, where_clause) = match &self.infra {
            Some(infra) => (
                type_params(&["S0"]),
                where_block(&[("S0", vec![infra.storage_trait.clone()])]),
            ),
            None => (String::new(), String::new()),
        };

        let mut context = Context::new();
        context.insert("generics", &generics);
        context.insert("where_clause", &where_clause);
        context.insert("input", &self.input.name);
        context.insert("output", &self.output.name);
        context.insert("error", &self.error_name());
        context.insert("name", &self.name);
        context.insert("body", &self.indent_body());
        context.insert("trait_name", &self.trait_name());
        context.insert("user_param", "_user");
        context.insert("transaction", &false);
        render("file/ai_use_case/impl.rs.tera", &context)
    }

    pub fn write(&mut self) -> Result<(), String> {
        let mut code = String::new();
        code.push_str(&self.gen_imports());
        code.push_str(&self.gen_error());
        code.push_str(&self.gen_trait());
        code.push_str(&self.gen_struct());
        code.push_str(&self.gen_impl());
        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to use case file: {}", e))
    }
}
