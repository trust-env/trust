use std::{fs::create_dir_all, path::PathBuf};

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::file::psql_storage_file::PsqlStorageFile},
    parser::{AllowedType, PsqlInfraService},
};

impl CodeGen {
    fn entity_storage_file_path(&self, psql: &PsqlInfraService, folder: &PathBuf) -> PathBuf {
        folder
            .join("psql")
            .join(format!("{}.rs", psql.entity.to_file_name()))
    }

    pub(super) fn gen_psql_storage(
        &mut self,
        context: &mut CodeGenContext,
        psql: PsqlInfraService,
        folder: PathBuf,
    ) -> Result<(), String> {
        let psql_folder_path = folder.join("psql");

        // Shared across every psql infra, so it may already exist by the
        // time a second (or third) one is generated.
        create_dir_all(psql_folder_path).map_err(|e| format!("Failed to create psql folder: {}", e))?;
        println!("Generating psql storage for entity: {:?}", psql.entity);
        match psql.entity {
            AllowedType::DefinedType(n) => match context.find_type(&n) {
                Some(rust_type) => {
                    let module_name = rust_type.to_file_name();
                    let mut file = PsqlStorageFile::new(rust_type.clone(), folder.clone())?;

                    file.write()?;
                    context.require_git_dependency(
                        "common",
                        "ssh://git@github.com/BH-World/common.git",
                    );
                    context.register_infra_entity_module(module_name);
                }
                None => {
                    return Err(format!("Entity type '{}' not found in context", n));
                }
            },
            _ => {
                return Err(format!(
                    "Unsupported entity type for psql storage: {:?}",
                    psql.entity
                ));
            }
        }
        Ok(())
    }
}
