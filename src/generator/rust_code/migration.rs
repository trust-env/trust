//! `CREATE TABLE` migrations for `psql` entities, written to the generated
//! crate's `migrations/` folder and applied by `main.rs` at startup.

use heck::ToSnakeCase;
use serde::Serialize;
use tera::Context;

use crate::generator::rust_code::{imports::DATE_TIME, render::render, types::RustType};

/// The table a `psql` entity is stored in — shared with the storage
/// codegen's own queries so the two can never drift apart.
pub fn table_name(entity_name: &str) -> String {
    format!("{}s", entity_name.to_snake_case())
}

pub fn migration_file_name(entity_name: &str) -> String {
    format!("{}.sql", table_name(entity_name))
}

pub fn seed_file_name(entity_name: &str) -> String {
    format!("{}.seed.sql", table_name(entity_name))
}

/// Maps a resolved field type to its Postgres column type, plus whether
/// it's nullable (only `Option<T>` is). Shared with the storage codegen,
/// which casts query parameters to these same column types.
pub(crate) fn sql_type(entity: &str, field: &str, t: &RustType) -> Result<(String, bool), String> {
    if t.name == "Option" {
        let inner = t
            .templates
            .first()
            .ok_or_else(|| format!("'{}.{}': empty Option", entity, field))?;
        let (sql, _) = sql_type(entity, field, inner)?;
        return Ok((sql, true));
    }
    let sql = match t.name.as_str() {
        "i64" => "BIGINT",
        "String" => "TEXT",
        "f64" => "DOUBLE PRECISION",
        "Decimal" => "NUMERIC",
        "Uuid" => "UUID",
        "bool" => "BOOLEAN",
        DATE_TIME => "TIMESTAMPTZ",
        other => {
            return Err(format!(
                "psql entity '{}': field '{}' has type '{}', which can't be stored in a column (only scalar types and Option<scalar> are supported)",
                entity, field, other
            ));
        }
    };
    Ok((sql.to_string(), false))
}

#[derive(Serialize)]
struct ColumnView {
    name: String,
    /// Full definition used in `CREATE TABLE`.
    definition: String,
    /// Bare type used by `ADD COLUMN IF NOT EXISTS` — always nullable,
    /// since a column added to a table that already has rows can't be
    /// `NOT NULL` without a default.
    sql_type: String,
    primary_key: bool,
}

#[derive(Serialize)]
struct IndexView {
    name: String,
    columns: String,
}

/// Postgres truncates identifiers past 63 bytes; keep the readable prefix
/// and make the tail unique with a short hash of the full name instead.
fn index_name(table: &str, columns: &[String]) -> String {
    let full = format!("{}_{}_idx", table, columns.join("_"));
    if full.len() <= 63 {
        return full;
    }
    let hash = full
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("{}_{:08x}", &full[..50], hash as u32)
}

/// Renders the migration for one entity: `CREATE TABLE IF NOT EXISTS`
/// (so a fresh database gets every column with its real nullability),
/// followed by one `ADD COLUMN IF NOT EXISTS` per non-key column (so a
/// field added to the `.tr` entity later shows up on an existing table).
///
/// `id: Int` becomes `BIGSERIAL PRIMARY KEY` — generated `save_one`
/// never inserts `id`, so the database must assign it.
///
/// `indexes` lists the column sets the generated code filters on (`filters
/// {}`, `owner:`, `auth {}` permission lookups — see
/// `CodeGenContext::register_psql_index`). Each becomes a btree index,
/// with `id` appended when the entity has one, since the generated `list`
/// always orders by `id`: `WHERE a = $1 ORDER BY id LIMIT n` then reads
/// exactly `n` index entries instead of sorting every match.
pub fn render_create_table(entity: &RustType, indexes: &[Vec<String>]) -> Result<String, String> {
    let mut columns = Vec::new();
    for field in &entity.fields {
        let (sql, nullable) = sql_type(&entity.name, &field.name, &field.field_type)?;
        let primary_key = field.name == "id";
        let definition = match (primary_key, sql.as_str()) {
            (true, "BIGINT") => "BIGSERIAL PRIMARY KEY".to_string(),
            (true, "UUID") => "UUID PRIMARY KEY DEFAULT gen_random_uuid()".to_string(),
            (true, other) => format!("{} PRIMARY KEY", other),
            (false, other) if nullable => other.to_string(),
            (false, other) => format!("{} NOT NULL", other),
        };
        columns.push(ColumnView {
            name: field.name.clone(),
            definition,
            sql_type: sql,
            primary_key,
        });
    }

    let table = table_name(&entity.name);
    let has_id = entity.fields.iter().any(|f| f.name == "id");
    let mut index_views: Vec<IndexView> = Vec::new();
    for index in indexes {
        let mut cols: Vec<String> = Vec::new();
        for col in index {
            if !entity.fields.iter().any(|f| &f.name == col) {
                return Err(format!(
                    "psql entity '{}': cannot index unknown field '{}'",
                    entity.name, col
                ));
            }
            if !cols.contains(col) {
                cols.push(col.clone());
            }
        }
        if has_id && !cols.iter().any(|c| c == "id") {
            cols.push("id".to_string());
        }
        // Only `id` left: the primary key already covers it.
        if cols.is_empty() || (cols.len() == 1 && cols[0] == "id") {
            continue;
        }
        let name = index_name(&table, &cols);
        if index_views.iter().any(|v| v.name == name) {
            continue;
        }
        index_views.push(IndexView {
            name,
            columns: cols.join(", "),
        });
    }

    let mut context = Context::new();
    context.insert("entity", &entity.name);
    context.insert("table", &table);
    context.insert("columns", &columns);
    context.insert("indexes", &index_views);
    Ok(render("migration/create_table.sql.tera", &context) + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::rust_code::types::RustField;

    fn field(name: &str, t: RustType) -> RustField {
        RustField {
            name: name.to_string(),
            field_type: t,
        }
    }

    #[test]
    fn maps_every_scalar_and_option_to_a_column() {
        let mut entity = RustType::primitive("CartItem");
        entity.add_field(field("id", RustType::primitive("i64")));
        entity.add_field(field("title", RustType::primitive("String")));
        entity.add_field(field("price", RustType::primitive("Decimal")));
        entity.add_field(field("paid", RustType::primitive("bool")));
        entity.add_field(field("created_at", RustType::primitive(DATE_TIME)));
        entity.add_field(field(
            "note",
            RustType::generic("Option", vec![RustType::primitive("String")], vec![]),
        ));

        let sql = render_create_table(&entity, &[]).unwrap();
        assert!(
            sql.contains("CREATE TABLE IF NOT EXISTS cart_items ("),
            "{sql}"
        );
        assert!(sql.contains("id BIGSERIAL PRIMARY KEY,"), "{sql}");
        assert!(sql.contains("title TEXT NOT NULL,"), "{sql}");
        assert!(sql.contains("price NUMERIC NOT NULL,"), "{sql}");
        assert!(sql.contains("paid BOOLEAN NOT NULL,"), "{sql}");
        assert!(sql.contains("created_at TIMESTAMPTZ NOT NULL,"), "{sql}");
        assert!(sql.contains("note TEXT\n);"), "{sql}");
        assert!(
            sql.contains("ALTER TABLE cart_items ADD COLUMN IF NOT EXISTS note TEXT;"),
            "{sql}"
        );
        assert!(!sql.contains("ADD COLUMN IF NOT EXISTS id "), "{sql}");
    }

    #[test]
    fn nested_struct_fields_are_rejected() {
        let mut entity = RustType::primitive("Order");
        entity.add_field(field("buyer", RustType::primitive("User")));
        let err = render_create_table(&entity, &[]).unwrap_err();
        assert!(err.contains("'buyer'") && err.contains("'User'"), "{err}");
    }

    #[test]
    fn filtered_columns_get_an_index_ending_with_id() {
        let mut entity = RustType::primitive("Rating");
        entity.add_field(field("id", RustType::primitive("i64")));
        entity.add_field(field("spot_id", RustType::primitive("i64")));
        entity.add_field(field("user_id", RustType::primitive("i64")));

        let sql = render_create_table(
            &entity,
            &[
                vec!["spot_id".to_string()],
                vec!["spot_id".to_string()],
                vec!["spot_id".to_string(), "user_id".to_string()],
                vec!["id".to_string()],
            ],
        )
        .unwrap();
        assert!(
            sql.contains("CREATE INDEX IF NOT EXISTS ratings_spot_id_id_idx ON ratings (spot_id, id);"),
            "{sql}"
        );
        assert!(
            sql.contains("CREATE INDEX IF NOT EXISTS ratings_spot_id_user_id_id_idx ON ratings (spot_id, user_id, id);"),
            "{sql}"
        );
        assert_eq!(sql.matches("CREATE INDEX").count(), 2, "{sql}");
    }

    #[test]
    fn indexing_an_unknown_field_is_an_error() {
        let mut entity = RustType::primitive("Rating");
        entity.add_field(field("id", RustType::primitive("i64")));
        let err = render_create_table(&entity, &[vec!["nope".to_string()]]).unwrap_err();
        assert!(err.contains("'nope'"), "{err}");
    }

    #[test]
    fn long_index_names_stay_within_postgres_limit() {
        let name = index_name("a_very_long_table_name_for_testing", &["first_long_column".to_string(), "second_long_column".to_string(), "id".to_string()]);
        assert!(name.len() <= 63, "{name}");
    }
}
