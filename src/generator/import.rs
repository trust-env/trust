use std::path::PathBuf;

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::types::RustType},
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
            crate::generator::domains::write_domain_type(context, rust_type, &folder)?;
        }
        Ok(())
    }
}
