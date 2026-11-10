use std::path::{Path, PathBuf};

use serde::Serialize;
use tera::Context;

use crate::generator::rust_code::{
    imports::{LocalImports, crate_use},
    migration,
    render::render,
    types::RustType,
};

/// The `Psql{Entity}StorageError` variants a use case's `errors {}` block
/// may map. `NotFound` also exists but always maps to the use case's own
/// `NotFound`.
pub const STORAGE_ERROR_VARIANTS: &[&str] = &["ConversionError", "DatabaseError"];

#[derive(Serialize)]
struct DataFieldView {
    name: String,
    /// Postgres column type (`save_many` casts each array to `TYPE[]`).
    sql_type: String,
    /// The field's Rust type (the `{Entity}Patch` field is `Option` of it).
    rust_type: String,
    /// `plain` / `nullable` when the value is bound through
    /// `sqlx::types::Json` (lists, nested structs); empty otherwise.
    json: &'static str,
}

#[derive(Serialize)]
struct ColumnView {
    name: String,
    sql_type: String,
}

/// `src/infrastructures/storage/psql/<entity>.rs`: the `Psql{Entity}Storage`
/// struct, its error type, the `{Entity}Storage` trait use cases are
/// generic over, and the `{Entity}Patch` struct `patch_one` takes.
pub struct PsqlStorageFile {
    entity: RustType,
    /// The first `unique: [...]` key of the infra — what `upsert_one`
    /// conflicts on.
    unique_key: Option<Vec<String>>,
    path: PathBuf,
}

impl PsqlStorageFile {
    pub fn new(entity: RustType, unique_key: Option<Vec<String>>, folder: PathBuf) -> Result<Self, String> {
        let path = folder.join("psql").join(format!("{}.rs", entity.to_file_name()));
        Ok(PsqlStorageFile { entity, unique_key, path })
    }

    pub fn error_name(entity: &str) -> String {
        format!("Psql{}StorageError", entity)
    }

    pub fn patch_name(entity: &str) -> String {
        format!("{}Patch", entity)
    }

    pub fn write(&mut self) -> Result<(), String> {
        let entity = &self.entity;
        let mut columns = Vec::new();
        for field in &entity.fields {
            let (sql_type, _) = migration::sql_type(&entity.name, &field.name, &field.field_type)?;
            columns.push(ColumnView { name: field.name.clone(), sql_type });
        }
        let column_type = |name: &str| {
            columns
                .iter()
                .find(|c| c.name == name)
                .map(|c| c.sql_type.clone())
                .unwrap_or_else(|| "TEXT".to_string())
        };
        let data_fields: Vec<DataFieldView> = entity
            .fields
            .iter()
            .filter(|f| f.name != "id")
            .map(|f| DataFieldView {
                name: f.name.clone(),
                sql_type: column_type(&f.name),
                rust_type: f.field_type.to_call_string(Vec::new()),
                json: match (f.field_type.is_json_wrapped(), f.field_type.is_option()) {
                    (true, true) => "nullable",
                    (true, false) => "plain",
                    _ => "",
                },
            })
            .collect();
        let owner_columns: Vec<String> = entity
            .fields
            .iter()
            .filter(|f| f.name != "id" && f.field_type.name == "i64")
            .map(|f| f.name.clone())
            .collect();
        let has_id = entity.fields.iter().any(|f| f.name == "id");
        let upsert_fields: Vec<String> = data_fields
            .iter()
            .filter(|f| !self.unique_key.as_ref().is_some_and(|k| k.contains(&f.name)))
            .map(|f| f.name.clone())
            .collect();
        if let Some(key) = &self.unique_key
            && upsert_fields.is_empty()
        {
            return Err(format!(
                "psql entity '{}': unique key [{}] covers every column, so an upsert has nothing to update",
                entity.name,
                key.join(", ")
            ));
        }

        // Field types the patch struct names (chrono, Decimal, nested
        // types, enums) plus the entity and the transaction types.
        let imports = LocalImports::from_fields(&entity.fields, &entity.name);
        let mut crate_items = vec![entity.name.clone(), "TrustTx".to_string(), "TrustTransactional".to_string()];
        crate_items.extend(imports.types.iter().cloned());
        let uses = imports.external_use_statements() + &crate_use(&crate_items);

        let mut context = Context::new();
        context.insert("uses", &uses);
        context.insert("entity", &entity.name);
        context.insert("error", &Self::error_name(&entity.name));
        context.insert("patch", &Self::patch_name(&entity.name));
        context.insert("trait_name", &format!("{}Storage", entity.name));
        context.insert("struct_name", &format!("Psql{}Storage", entity.name));
        context.insert("table", &migration::table_name(&entity.name));
        context.insert("all_columns", &entity.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>());
        context.insert("columns", &columns);
        context.insert("id_field", "id");
        context.insert("has_id", &has_id);
        context.insert("next", &(data_fields.len() + 1));
        context.insert("next_owner", &(data_fields.len() + 2));
        context.insert("data_fields", &data_fields);
        context.insert("owner_columns", &owner_columns);
        context.insert("unique_key", &self.unique_key);
        context.insert("upsert_fields", &upsert_fields);
        let code = render("file/psql_storage/storage.rs.tera", &context);
        std::fs::write(&self.path, code)
            .map_err(|e| format!("Failed to write entity storage file: {}", e))
    }
}

/// Writes the crate's shared transaction module (`storage/psql/trust_tx.rs`).
pub fn write_transaction_module(storage_folder: &Path) -> Result<(), String> {
    std::fs::write(
        storage_folder.join("psql").join("trust_tx.rs"),
        render("file/psql_storage/trust_tx.rs.tera", &Context::new()),
    )
    .map_err(|e| format!("Failed to write the transaction module: {}", e))
}
