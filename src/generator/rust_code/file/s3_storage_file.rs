use std::{fs::File, io::Write, path::PathBuf};

use crate::generator::rust_code::render::render;
use tera::Context;

/// Writes `src/infrastructures/storage/s3/{entity}.rs`: a `S3{Entity}Storage`
/// struct wrapping an `s3` crate `Bucket`. Unlike `PsqlStorageFile`, the
/// entity's own fields never appear in this file — an `s3 {}` infra's
/// `entity:` is metadata-only (see `S3InfraService`'s doc comment), so
/// there's nothing here to resolve beyond the entity's name.
pub struct S3StorageFile {
    entity_name: String,
    module_name: String,
    file: File,
}

impl S3StorageFile {
    pub fn new(entity_name: String, module_name: String, folder: PathBuf) -> Result<Self, String> {
        let file_path = folder.join("s3").join(format!("{}.rs", module_name));

        Ok(S3StorageFile {
            file: File::create(&file_path)
                .map_err(|e| format!("Failed to create s3 storage file: {}", e))?,
            entity_name,
            module_name,
        })
    }

    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    pub fn struct_name(&self) -> String {
        format!("S3{}Storage", self.entity_name)
    }

    pub fn write(&mut self) -> Result<(), String> {
        let mut context = Context::new();
        context.insert("struct_name", &self.struct_name());
        let code = render("file/s3_storage/storage.rs.tera", &context);

        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to s3 storage file: {}", e))
    }
}
