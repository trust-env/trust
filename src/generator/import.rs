use std::{fs::File, io::Write, path::PathBuf};

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::{RustCode, imports::LocalImports, types::RustType}},
    lexer::Lexer,
    parser::{Ast, ImportDeclaration, ImportItem, ModuleKind, Parser, TypeDeclaration},
};

impl CodeGen {
    fn requested_import_names(items: &ImportItem) -> Vec<&str> {
        match items {
            ImportItem::All(name) => vec![name.as_str()],
            ImportItem::Specific(names) => names.iter().map(String::as_str).collect(),
        }
    }

    fn load_module_asts(module: &ModuleKind) -> Result<Vec<Ast>, String> {
        let path = match module {
            ModuleKind::Path(path) => path,
            ModuleKind::Git(url) => {
                return Err(format!("Git imports are not supported yet: {}", url));
            }
        };
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read imported module {:?}: {}", path, e))?;

        Parser::new(Lexer::new(&content).run()?).run()
    }

    fn find_exported_type<'a>(
        module_asts: &'a [Ast],
        name: &str,
    ) -> Option<&'a TypeDeclaration> {
        module_asts.iter().find_map(|ast| match ast {
            Ast::TypeDecl(t) if t.export_name() == Some(name) => Some(t),
            _ => None,
        })
    }

    /// Resolves every type an `import` line requests, without registering
    /// dependencies or writing any file. Used by a pre-pass (so anything
    /// referencing an imported type — e.g. an infra's `entity:` — can
    /// resolve it regardless of where the `import` sits in the `.tr` file)
    /// and by `generate_import` itself, which redoes this same resolution
    /// before writing the domain file; the duplicate work is cheap and
    /// idempotent.
    pub(super) fn resolve_import_types(
        &self,
        context: &mut CodeGenContext,
        import: &ImportDeclaration,
    ) -> Result<Vec<RustType>, String> {
        let module_asts = Self::load_module_asts(&import.module)?;
        let mut types = Vec::new();

        for name in Self::requested_import_names(&import.items) {
            let t_decl = Self::find_exported_type(&module_asts, name).ok_or_else(|| {
                format!(
                    "No export named '{}' found in imported module {:?}",
                    name, import.module
                )
            })?;

            let mut rust_type = self.resolve_type_declaration(context, t_decl)?;
            rust_type.name = name.to_string();
            types.push(rust_type);
        }
        Ok(types)
    }

    pub(super) fn generate_import(
        &mut self,
        context: &mut CodeGenContext,
        import: ImportDeclaration,
        folder: PathBuf,
    ) -> Result<(), String> {
        for rust_type in self.resolve_import_types(context, &import)? {
            let domain_file_path = folder.join(format!("{}.rs", rust_type.to_file_name()));
            let mut domain_file = File::create(&domain_file_path)
                .map_err(|e| format!("Failed to create domain file: {}", e))?;
            let imports = LocalImports::from_fields(&rust_type.fields, &rust_type.name);
            if imports.needs_uuid {
                context.require_version_dependency_with_features("uuid", "1", vec!["serde"]);
            }
            if imports.needs_decimal {
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
            let mut derive = "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize".to_string();
            if context.is_storage_entity(&rust_type.name) {
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
                derive.push_str(", sqlx::FromRow");
            }
            derive.push_str(")]\n");
            let code = imports.to_use_statements() + &derive + &rust_type.to_rust_code();

            domain_file
                .write_all(code.as_bytes())
                .map_err(|e| format!("Failed to write to domain file: {}", e))?;

            context.register_domain_module(rust_type.to_file_name());
            context.add_type(rust_type);
        }
        Ok(())
    }
}
