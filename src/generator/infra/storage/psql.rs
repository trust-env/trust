use std::{
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::{
            file::psql_storage_file::{PsqlStorageFile, write_transaction_module},
            migration,
        },
    },
    parser::{AllowedType, PsqlInfraService},
};

impl CodeGen {
    /// Writes `migrations/<table>.sql` (and `<table>.seed.sql`, copied from
    /// the infra's `seed:` path, resolved against the working directory
    /// like every other `.tr` path, and `<table>.fk.sql` for foreign keys)
    /// into the generated crate.
    fn write_migrations(
        context: &CodeGenContext,
        entity: &crate::generator::rust_code::types::RustType,
        indexes: &[Vec<String>],
        psql: &PsqlInfraService,
        crate_root: &Path,
    ) -> Result<(), String> {
        let dir = crate_root.join("migrations");
        create_dir_all(&dir).map_err(|e| format!("Failed to create migrations folder: {}", e))?;
        let options = migration::TableOptions {
            unique: &psql.unique,
            renamed: &psql.renamed,
            dropped: &psql.dropped,
        };
        std::fs::write(
            dir.join(migration::migration_file_name(&entity.name)),
            migration::render_table(entity, indexes, &options)?,
        )
        .map_err(|e| format!("Failed to write migration for '{}': {}", entity.name, e))?;
        if let Some(seed) = &psql.seed {
            let content = std::fs::read_to_string(seed).map_err(|e| {
                format!(
                    "Failed to read seed file '{}' for entity '{}': {}",
                    seed, entity.name, e
                )
            })?;
            std::fs::write(dir.join(migration::seed_file_name(&entity.name)), content)
                .map_err(|e| format!("Failed to write seed for '{}': {}", entity.name, e))?;
        }
        if !psql.references.is_empty() {
            let mut keys = Vec::new();
            for reference in &psql.references {
                let target = context
                    .find_infra_entity(&reference.infra)
                    .filter(|_| context.find_infra_target_address(&reference.infra).is_none())
                    .ok_or_else(|| {
                        format!(
                            "psql entity '{}': 'references' {} -> '{}', which isn't a psql infra",
                            entity.name, reference.field, reference.infra
                        )
                    })?;
                let AllowedType::DefinedType(target) = target else {
                    return Err(format!("Infra '{}' entity must be a declared type", reference.infra));
                };
                keys.push(migration::ForeignKeySpec {
                    column: reference.field.clone(),
                    ref_table: migration::table_name(target),
                    on_delete: reference.on_delete.clone(),
                });
            }
            std::fs::write(
                dir.join(migration::foreign_keys_file_name(&entity.name)),
                migration::render_foreign_keys(entity, &keys)?,
            )
            .map_err(|e| format!("Failed to write foreign keys for '{}': {}", entity.name, e))?;
        }
        Ok(())
    }

    fn entity_storage_file_path(&self, psql: &PsqlInfraService, folder: &PathBuf) -> PathBuf {
        folder
            .join("psql")
            .join(format!("{}.rs", psql.entity.to_file_name()))
    }

    pub(super) fn gen_psql_storage(
        &mut self,
        context: &mut CodeGenContext,
        infra_name: &str,
        psql: PsqlInfraService,
        folder: PathBuf,
        crate_root: &Path,
    ) -> Result<(), String> {
        let psql_folder_path = folder.join("psql");

        // Shared across every psql infra, so it may already exist by the
        // time a second (or third) one is generated.
        create_dir_all(psql_folder_path).map_err(|e| format!("Failed to create psql folder: {}", e))?;
        match &psql.entity {
            AllowedType::DefinedType(n) => match context.find_type(n).cloned() {
                Some(rust_type) => {
                    let rust_type = &rust_type;
                    let module_name = rust_type.to_file_name();
                    let mut file = PsqlStorageFile::new(rust_type.clone(), psql.unique.first().cloned(), folder.clone())?
                        .with_schema_rules(&psql.defaults, &psql.stamps, context.app_tz().is_some())?;

                    file.write()?;
                    if !context.has_transaction_module() {
                        write_transaction_module(&folder)?;
                        context.register_infra_entity_module("trust_tx".to_string());
                        context.mark_transaction_module();
                    }
                    let indexes = context.find_psql_indexes(infra_name).to_vec();
                    Self::write_migrations(context, rust_type, &indexes, &psql, crate_root)?;
                    context.require_git_dependency(
                        "common",
                        crate::generator::RUNTIME_GIT_URL,
                    );
                    // Row snapshots (`to_jsonb`) for journaled runs.
                    context.require_version_dependency("serde_json", "1");
                    context.require_version_dependency_with_features("sqlx", "0.9.0", vec!["json"]);
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
