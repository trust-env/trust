use std::{fs::File, io::Write, path::PathBuf};

use heck::ToSnakeCase;
use serde::Serialize;
use tera::Context;

use crate::generator::rust_code::fragments::{indented_body, params as fragment_params, type_params, where_block};
use crate::generator::rust_code::render::render;

/// One `infra:` of a `code {}` use case, as the generated struct holds it.
pub enum CodeDependency {
    /// A `psql`/`grpc_client` infra: a generic parameter bounded by the
    /// entity's storage trait, like CRUD use cases.
    Storage {
        infra_name: String,
        storage_trait: String,
        storage_error: String,
        /// A `psql` storage: also bound by `TrustTransactional`, so the body
        /// (or `transaction: true`) can run it inside a transaction.
        transactional: bool,
    },
    /// An `s3`/`http_client` infra: its concrete struct (neither implements
    /// a shared trait to be generic over).
    Concrete {
        infra_name: String,
        type_name: String,
    },
}

impl CodeDependency {
    fn field(&self) -> String {
        match self {
            CodeDependency::Storage { infra_name, .. }
            | CodeDependency::Concrete { infra_name, .. } => infra_name.to_snake_case(),
        }
    }
}

/// A use case whose `execute` body is hand-written Rust read from a file
/// (see `CodeSpec`) — everything around it (error enum, trait, struct,
/// constructor, `UseCase` impl) is generated exactly like the other kinds.
pub struct CodeUseCaseFile {
    pub name: String,
    pub input: String,
    pub output: String,
    pub source_file: String,
    pub body: String,
    pub dependencies: Vec<CodeDependency>,
    pub error_variants: Vec<String>,
    /// `transaction: true`: the whole body runs in one transaction.
    pub transaction: bool,
}

impl CodeUseCaseFile {
    fn error_name(&self) -> String {
        format!("{}Error", self.name)
    }

    fn trait_name(&self) -> String {
        format!("{}Trait", self.name)
    }

    /// `S0, S1, ...` for each storage dependency, in declaration order.
    fn generic_params(&self) -> Vec<(String, Vec<String>)> {
        self.dependencies
            .iter()
            .filter_map(|d| match d {
                CodeDependency::Storage { storage_trait, transactional, .. } => {
                    let mut bounds = vec![storage_trait.clone()];
                    if *transactional {
                        bounds.push("TrustTransactional".to_string());
                    }
                    Some(bounds)
                }
                CodeDependency::Concrete { .. } => None,
            })
            .enumerate()
            .map(|(i, bounds)| (format!("S{}", i), bounds))
            .collect()
    }

    fn generics(&self) -> String {
        let names: Vec<String> = self.generic_params().into_iter().map(|(p, _)| p).collect();
        type_params(&names)
    }

    fn where_clause(&self) -> String {
        where_block(&self.generic_params())
    }

    fn gen_error(&self) -> String {
        let mut storage_errors: Vec<&str> = Vec::new();
        for dependency in &self.dependencies {
            if let CodeDependency::Storage { storage_error, .. } = dependency {
                if !storage_errors.contains(&storage_error.as_str()) {
                    storage_errors.push(storage_error);
                }
            }
        }
        let mut context = Context::new();
        context.insert("name", &self.error_name());
        context.insert("variants", &self.error_variants);
        context.insert("storage_errors", &storage_errors);
        render("file/code_use_case/error.rs.tera", &context)
    }

    fn gen_struct(&self) -> String {
        #[derive(Serialize)]
        struct FieldView {
            field: String,
            field_type: String,
        }

        let mut generic_index = 0;
        let fields: Vec<FieldView> = self
            .dependencies
            .iter()
            .map(|d| {
                let field_type = match d {
                    CodeDependency::Storage { .. } => {
                        generic_index += 1;
                        format!("S{}", generic_index - 1)
                    }
                    CodeDependency::Concrete { type_name, .. } => type_name.clone(),
                };
                FieldView {
                    field: d.field(),
                    field_type,
                }
            })
            .collect();
        let params: Vec<(&str, &str)> = fields.iter().map(|f| (f.field.as_str(), f.field_type.as_str())).collect();
        let params = fragment_params(&params);
        let ctor_fields: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();

        let mut context = Context::new();
        context.insert("name", &self.name);
        context.insert("generics", &self.generics());
        context.insert("where_clause", &self.where_clause());
        context.insert("fields", &fields);
        context.insert("params", &params);
        context.insert("ctor_fields", &ctor_fields);
        render("file/code_use_case/struct.rs.tera", &context)
    }

    fn gen_trait(&self) -> String {
        let mut context = Context::new();
        context.insert("trait_name", &self.trait_name());
        context.insert("input", &self.input);
        context.insert("output", &self.output);
        context.insert("error", &self.error_name());
        render("file/ai_use_case/trait.rs.tera", &context)
    }

    /// The hand-written body, preceded by a use of `user` so an
    /// owner-agnostic body doesn't trigger an unused-variable warning.
    fn indented_body(&self) -> String {
        let mut lines = vec!["let _ = user;"];
        lines.extend(
            self.body
                .trim_end()
                .lines()
                .map(|line| if line.trim().is_empty() { "" } else { line }),
        );
        indented_body(&lines)
    }

    fn gen_impl(&self) -> String {
        let mut context = Context::new();
        context.insert("generics", &self.generics());
        context.insert("where_clause", &self.where_clause());
        context.insert("input", &self.input);
        context.insert("output", &self.output);
        context.insert("error", &self.error_name());
        context.insert("name", &self.name);
        context.insert("body", &self.indented_body());
        context.insert("trait_name", &self.trait_name());
        context.insert("user_param", "user");
        #[derive(Serialize)]
        struct TxFieldView {
            field: String,
            storage: bool,
        }
        let tx_fields: Vec<TxFieldView> = self
            .dependencies
            .iter()
            .map(|d| TxFieldView {
                field: d.field(),
                storage: matches!(d, CodeDependency::Storage { .. }),
            })
            .collect();
        context.insert("transaction", &self.transaction);
        context.insert(
            "tx_first",
            &tx_fields.iter().find(|f| f.storage).map(|f| f.field.clone()).unwrap_or_default(),
        );
        context.insert("tx_fields", &tx_fields);
        render("file/ai_use_case/impl.rs.tera", &context)
    }

    pub fn write(&self, folder: &PathBuf) -> Result<(), String> {
        let mut context = Context::new();
        context.insert("file", &self.source_file);
        let mut code = render("file/code_use_case/header.rs.tera", &context);
        code.push_str(&self.gen_error());
        code.push_str(&self.gen_trait());
        code.push_str(&self.gen_struct());
        code.push_str(&self.gen_impl());

        let path = folder.join(format!("{}.rs", self.name.to_snake_case()));
        File::create(&path)
            .and_then(|mut f| f.write_all(code.as_bytes()))
            .map_err(|e| format!("Failed to write use case file {:?}: {}", path, e))
    }
}
