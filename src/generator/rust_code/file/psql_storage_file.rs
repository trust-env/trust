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

/// The entity's columns and its non-`id` fields as bound by an INSERT.
fn column_views(entity: &RustType) -> Result<(Vec<ColumnView>, Vec<DataFieldView>), String> {
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
    Ok((columns, data_fields))
}

/// The non-`id` fields of `entity` as an INSERT binds them, as JSON values
/// for a template (`name`, `sql_type`, `rust_type`, `json`).
pub fn insert_field_views(entity: &RustType) -> Result<Vec<serde_json::Value>, String> {
    let (_, data_fields) = column_views(entity)?;
    Ok(data_fields.iter().map(|f| serde_json::to_value(f).expect("serializable")).collect())
}

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
    defaults: Vec<DefaultView>,
    stamps: Vec<StampView>,
}

impl PsqlStorageFile {
    pub fn new(entity: RustType, unique_key: Option<Vec<String>>, folder: PathBuf) -> Result<Self, String> {
        let path = folder.join("psql").join(format!("{}.rs", entity.to_file_name()));
        Ok(PsqlStorageFile { entity, unique_key, path, defaults: Vec::new(), stamps: Vec::new() })
    }

    /// `defaults {}` / `stamps {}` of the infra.
    pub fn with_schema_rules(
        mut self,
        defaults: &[(String, crate::parser::DefaultValue)],
        stamps: &[crate::parser::Stamp],
        local_tz: bool,
    ) -> Result<Self, String> {
        for (field, value) in defaults {
            let mut expr = default_expr(&self.entity, field, value)?;
            if local_tz {
                // `today()` is the app's local date (`config { tz }`).
                expr = expr.replace("chrono::Utc::now().date_naive()", "crate::tz::today()");
            }
            self.defaults.push(DefaultView { field: field.clone(), expr });
        }
        for stamp in stamps {
            self.stamps.push(StampView { set_sql: stamp_set_sql(&self.entity, stamp)? });
        }
        Ok(self)
    }

    pub fn error_name(entity: &str) -> String {
        format!("Psql{}StorageError", entity)
    }

    pub fn patch_name(entity: &str) -> String {
        format!("{}Patch", entity)
    }

    pub fn write(&mut self) -> Result<(), String> {
        let entity = &self.entity;
        let (columns, data_fields) = column_views(entity)?;
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
        let mut crate_items = vec![
            entity.name.clone(),
            "Query".to_string(),
            "TrustTx".to_string(),
            "TrustTransactional".to_string(),
            "TrustJournal".to_string(),
            "TrustChange".to_string(),
        ];
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
        context.insert("defaults", &self.defaults);
        context.insert("stamps", &self.stamps);
        context.insert("entity_snake", &heck::ToSnakeCase::to_snake_case(entity.name.as_str()));
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

/// One `defaults {}` entry as the storage's `apply_<entity>_defaults` writes it.
#[derive(Serialize, Clone)]
pub struct DefaultView {
    pub field: String,
    /// A Rust expression of the field's type (may read `entity`'s fields).
    pub expr: String,
}

/// One `stamps {}` entry: the SQL `SET` fragment for the column.
#[derive(Serialize, Clone)]
pub struct StampView {
    pub set_sql: String,
}

pub(crate) fn literal_expr(entity: &str, field: &str, t: &RustType, value: &serde_json::Value) -> Result<String, String> {
    use serde_json::Value;
    if t.is_option() {
        return match value {
            Value::Null => Ok("None".to_string()),
            other => Ok(format!("Some({})", literal_expr(entity, field, t.unwrap_option(), other)?)),
        };
    }
    let mismatch = || format!("defaults of '{}': '{}' doesn't fit field '{}' ({})", entity, value, field, t.name);
    match (t.name.as_str(), value) {
        ("String", Value::String(s)) => Ok(format!("{:?}.to_string()", s)),
        ("i64", Value::Number(n)) if n.is_i64() => Ok(n.to_string()),
        ("f64", Value::Number(n)) => Ok(format!("{}_f64", n)),
        ("bool", Value::Bool(b)) => Ok(b.to_string()),
        (_, Value::String(s)) if t.is_enum() => match &t.kind {
            crate::generator::rust_code::types::TypeKind::Enum(values) if values.contains(s) => {
                Ok(format!("{}::{}", t.name, crate::generator::types::enum_variant_ident(s)))
            }
            _ => Err(mismatch()),
        },
        ("String" | "i64" | "f64" | "bool", _) => Err(mismatch()),
        _ => Ok(format!(
            "serde_json::from_str({:?}).expect(\"default of {}.{} checked by trust\")",
            value.to_string(),
            entity,
            field
        )),
    }
}

/// The Rust expression a `defaults {}` value stands for.
pub fn default_expr(entity: &RustType, field: &str, value: &crate::parser::DefaultValue) -> Result<String, String> {
    use crate::parser::{DefaultArg, DefaultValue};
    let target = entity
        .fields
        .iter()
        .find(|f| f.name == field)
        .ok_or_else(|| format!("defaults of '{}': unknown field '{}'", entity.name, field))?;
    let t = &target.field_type;
    let wrap = |expr: String| if t.is_option() { format!("Some({})", expr) } else { expr };
    match value {
        DefaultValue::Literal(v) => literal_expr(&entity.name, field, t, v),
        DefaultValue::Now => match t.unwrap_option().name.as_str() {
            crate::generator::rust_code::imports::DATE_TIME => Ok(wrap("chrono::Utc::now()".to_string())),
            "NaiveDate" => Ok(wrap("chrono::Utc::now().date_naive()".to_string())),
            other => Err(format!("defaults of '{}': now() doesn't fit '{}' ({})", entity.name, field, other)),
        },
        DefaultValue::Today => match t.unwrap_option().name.as_str() {
            "NaiveDate" => Ok(wrap("chrono::Utc::now().date_naive()".to_string())),
            other => Err(format!("defaults of '{}': today() doesn't fit '{}' ({})", entity.name, field, other)),
        },
        DefaultValue::Call(name, args) => {
            let mut rendered = Vec::new();
            for arg in args {
                rendered.push(match arg {
                    DefaultArg::Field(f) => {
                        if !entity.fields.iter().any(|ef| &ef.name == f) {
                            return Err(format!("defaults of '{}': {}(...) reads unknown field '{}'", entity.name, name, f));
                        }
                        format!("&entity.{}", f)
                    }
                    DefaultArg::Now => "chrono::Utc::now()".to_string(),
                    DefaultArg::Today => "chrono::Utc::now().date_naive()".to_string(),
                    DefaultArg::Literal(serde_json::Value::String(s)) => format!("{:?}", s),
                    DefaultArg::Literal(other) => other.to_string(),
                });
            }
            Ok(format!("crate::{}({})", name, rendered.join(", ")))
        }
    }
}

fn sql_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// The `SET` fragment of a `stamps {}` entry.
pub fn stamp_set_sql(entity: &RustType, stamp: &crate::parser::Stamp) -> Result<String, String> {
    let field = entity
        .fields
        .iter()
        .find(|f| f.name == stamp.field)
        .ok_or_else(|| format!("stamps of '{}': unknown field '{}'", entity.name, stamp.field))?;
    if !field.field_type.is_option() {
        return Err(format!("stamps of '{}': '{}' must be optional (it is null while the condition is false)", entity.name, stamp.field));
    }
    let now = match field.field_type.unwrap_option().name.as_str() {
        crate::generator::rust_code::imports::DATE_TIME => "now()",
        "NaiveDate" => "CURRENT_DATE",
        other => return Err(format!("stamps of '{}': '{}' must be a DateTime or Date, not {}", entity.name, stamp.field, other)),
    };
    if !entity.fields.iter().any(|f| f.name == stamp.when_field) {
        return Err(format!("stamps of '{}': unknown field '{}'", entity.name, stamp.when_field));
    }
    let values: Vec<String> = stamp.values.iter().map(|v| sql_quote(v)).collect();
    let condition = format!(
        "{}{} IN ({})",
        stamp.when_field,
        if stamp.negated { " NOT" } else { "" },
        values.join(", ")
    );
    Ok(format!(
        "{f} = CASE WHEN {c} THEN COALESCE({f}, {now}) ELSE NULL END",
        f = stamp.field,
        c = condition,
        now = now
    ))
}
