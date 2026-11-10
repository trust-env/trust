use std::path::PathBuf;

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::types::RustType},
    parser::{Ast, ImportDeclaration, ImportItem, ModuleKind, TypeDeclaration},
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
        crate::loader::load_project(path, &[])
            .map_err(|e| format!("Failed to load imported module {:?}: {}", path, e))
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
        _context: &mut CodeGenContext,
        import: &ImportDeclaration,
    ) -> Result<Vec<RustType>, String> {
        let module_asts = Self::load_module_asts(&import.module)?;

        // The module's types resolve against the module itself (and what it
        // imports), not against the importing file.
        let mut module_context = CodeGenContext::new();
        for ast in &module_asts {
            if let Ast::Import(nested) = ast {
                for t in self.resolve_import_types(&mut module_context, nested)? {
                    module_context.add_type(t);
                }
            }
        }
        let decls: Vec<&TypeDeclaration> = module_asts
            .iter()
            .filter_map(|ast| match ast {
                Ast::TypeDecl(t) => Some(t),
                _ => None,
            })
            .collect();
        self.resolve_type_declarations(&mut module_context, &decls)
            .map_err(|e| format!("in imported module {:?}: {}", import.module, e))?;

        // The requested types, plus every type they mention (generated too,
        // under their own names).
        let mut types: Vec<RustType> = Vec::new();
        for name in Self::requested_import_names(&import.items) {
            let t_decl = Self::find_exported_type(&module_asts, name).ok_or_else(|| {
                format!(
                    "No export named '{}' found in imported module {:?}",
                    name, import.module
                )
            })?;
            let mut rust_type = module_context
                .find_type(t_decl.name())
                .cloned()
                .ok_or_else(|| format!("type '{}' not resolved in {:?}", t_decl.name(), import.module))?;
            let mut mentioned = Vec::new();
            crate::generator::schema::collect_named_types(&rust_type, &mut mentioned);
            rust_type.name = name.to_string();
            if !types.iter().any(|t| t.name == rust_type.name) {
                types.push(rust_type);
            }
            for dependency in mentioned.into_iter().skip(1) {
                if !types.iter().any(|t| t.name == dependency.name) {
                    types.push(dependency);
                }
            }
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
            crate::generator::domains::write_domain_type(context, rust_type, &folder)?;
        }
        Ok(())
    }
}
