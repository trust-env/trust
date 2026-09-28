use crate::generator::rust_code::RustCode;
use std::io::Write;
use std::{fs::File, path::PathBuf};

use heck::ToSnakeCase;

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::{imports::LocalImports, render::render}},
    parser::TypeDeclaration,
};
use tera::Context;

impl CodeGen {
    fn type_name(&self, t: &TypeDeclaration) -> String {
        match t {
            TypeDeclaration::Struct(s) => s.name.clone(),
            TypeDeclaration::TypeAlias(a) => a.name.clone(),
        }
    }

    pub(super) fn gen_domain(
        &mut self,
        context: &mut CodeGenContext,
        t_decl: &TypeDeclaration,
        folder: PathBuf,
    ) -> Result<(), String> {
        let t = self.resolve_type_declaration(context, t_decl)?;

        context.add_type(t.clone());
        context.register_domain_module(t.to_file_name());

        let domain_file_path = folder.join(format!("{}.rs", t.name.to_snake_case()));
        let mut domain_file = File::create(domain_file_path)
            .map_err(|e| format!("Failed to create domain file: {}", e))?;
        let imports = LocalImports::from_fields(&t.fields, &t.name);
        if imports.needs_uuid {
            context.require_version_dependency_with_features("uuid", "1", vec!["serde"]);
        }
        if imports.needs_decimal {
            // "serde-str" forces Decimal to (de)serialize as a JSON string
            // rather than a float, so money/precision fields never round-trip
            // through IEEE-754 float imprecision over the wire.
            context.require_version_dependency_with_features(
                "rust_decimal",
                "1",
                vec!["serde-str"],
            );
        }

        if imports.needs_chrono {
            context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
        }

        context.require_version_dependency_with_features("serde", "1", vec!["derive"]);
        let is_storage_entity = context.is_storage_entity(&t.name);
        if is_storage_entity {
            let mut features = vec!["runtime-tokio", "tls-rustls", "postgres"];
            if imports.needs_uuid {
                features.push("uuid");
            }
            if imports.needs_decimal {
                features.push("rust_decimal");
            }
            if imports.needs_chrono {
                features.push("chrono");
            }
            context.require_version_dependency_with_features("sqlx", "0.9.0", features);
        }
        let mut derive_context = Context::new();
        derive_context.insert("is_storage_entity", &is_storage_entity);
        let derive = render("domains/derive.rs.tera", &derive_context);
        let code = imports.to_use_statements() + &derive + &t.to_rust_code();

        domain_file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to domain file: {}", e))?;
        Ok(())
    }
}
