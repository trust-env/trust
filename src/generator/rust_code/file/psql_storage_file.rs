use std::io::Write;
use std::{fs::File, path::PathBuf};

use crate::generator::rust_code::_enum::Enum;
use crate::generator::rust_code::_trait::Trait;
use crate::generator::rust_code::{
    RustCode,
    imports::{common_use, crate_use},
    render::render,
    service::psql_storage::PsqlStorageService,
    types::RustType,
};
use tera::Context;

/// The fixed set of variants `Psql{Entity}StorageError` always has (see
/// `error_enum_code` below). Kept in one place so `errors {}` mapping
/// validation and codegen can agree on what's a valid mapping source.
pub const STORAGE_ERROR_VARIANTS: &[&str] = &["ConversionError", "DatabaseError"];

enum InFileType {
    Struct(RustType),
    Enum(Enum),
    Trait(Trait),
}

pub struct PsqlStorageFile {
    entity: RustType,
    error_name: String,
    types: Vec<InFileType>,
    service: PsqlStorageService,
    file: File,
}

impl PsqlStorageFile {
    fn file_name(folder: &PathBuf, e: &RustType) -> PathBuf {
        folder.join("psql").join(format!("{}.rs", e.to_file_name()))
    }

    fn error_name(e: &RustType) -> String {
        format!("Psql{}StorageError", e.name)
    }

    fn storage_trait(e: RustType, error_name: &str) -> Trait {
        Trait {
            name: format!("{}Storage", e.to_call_string(vec![])),
            methods: vec![],
            templates: vec![],
            implements: vec![
                Trait::storage(e.clone(), error_name),
                Trait::send(),
                Trait::sync(),
            ],
        }
    }

    fn error_enum_code(error_name: &str) -> String {
        let mut context = Context::new();
        context.insert("name", error_name);
        render("file/psql_storage/error_enum.rs.tera", &context)
    }

    pub fn new(e: RustType, folder: PathBuf) -> Result<Self, String> {
        let error_name = Self::error_name(&e);

        Ok(PsqlStorageFile {
            types: vec![InFileType::Trait(Self::storage_trait(e.clone(), &error_name))],
            service: PsqlStorageService::new(e.clone(), Self::storage_trait(e.clone(), &error_name)),
            file: File::create(Self::file_name(&folder, &e))
                .map_err(|err| format!("Failed to create entity storage file: {}", err))?,
            entity: e,
            error_name,
        })
    }

    pub fn write(&mut self) -> Result<(), String> {
        let mut code = String::new();

        code.push_str(&common_use(&[
            "SQLConnector",
            "Storage",
            "EntityId",
            "StorageListQuery",
        ]));
        code.push_str(&crate_use(&[self.entity.name.clone()]));
        code.push('\n');
        code.push_str(&Self::error_enum_code(&self.error_name));
        if !self.types.is_empty() {
            for t in &self.types {
                match t {
                    InFileType::Trait(t) => code.push_str(&t.to_rust_code()),
                    _ => {}
                }
                code.push_str("\n");
            }
        }
        code.push_str("#[derive(Clone)]\n");
        code.push_str(&self.service.struct_code());
        code.push_str(&self.service.ctor_code());
        code.push_str(&self.service.trait_impls_code());
        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to entity storage file: {}", e))?;
        Ok(())
    }
}
