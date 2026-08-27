use std::{fs::create_dir_all, path::PathBuf};

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::file::s3_storage_file::S3StorageFile},
    parser::{AllowedType, S3InfraService},
};

impl CodeGen {
    pub(super) fn gen_s3_storage(
        &mut self,
        context: &mut CodeGenContext,
        cfg: S3InfraService,
        folder: PathBuf,
    ) -> Result<(), String> {
        let s3_folder_path = folder.join("s3");

        // Shared across every s3 infra, so it may already exist by the
        // time a second one is generated (same pattern as `psql`).
        create_dir_all(&s3_folder_path)
            .map_err(|e| format!("Failed to create s3 folder: {}", e))?;

        match &cfg.entity {
            AllowedType::DefinedType(n) => match context.find_type(n) {
                Some(rust_type) => {
                    let module_name = rust_type.to_file_name();
                    let mut file =
                        S3StorageFile::new(rust_type.name.clone(), module_name.clone(), folder)?;

                    file.write()?;
                    context.require_version_dependency("rust-s3", "0.37");
                    context.register_s3_entity_module(module_name);
                    Ok(())
                }
                None => Err(format!("Entity type '{}' not found in context", n)),
            },
            _ => Err(format!(
                "Unsupported entity type for s3 storage: {:?}",
                cfg.entity
            )),
        }
    }
}
