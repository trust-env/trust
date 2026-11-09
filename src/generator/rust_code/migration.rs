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
    if t.is_json_wrapped() || t.name == crate::generator::types::JSON_VALUE {
        return Ok(("JSONB".to_string(), false));
    }
    if t.is_enum() {
        return Ok(("TEXT".to_string(), false));
    }
    let sql = match t.name.as_str() {
        "i64" => "BIGINT",
        "NaiveDate" => "DATE",
        "NaiveTime" => "TIME",
        "String" => "TEXT",
        "f64" => "DOUBLE PRECISION",
        "Decimal" => "NUMERIC",
        "Uuid" => "UUID",
        "bool" => "BOOLEAN",
        DATE_TIME => "TIMESTAMPTZ",
        other => {
            return Err(format!(
                "psql entity '{}': field '{}' has type '{}', which can't be stored in a column (declare it as a struct, List<...> or Json to store it as JSONB)",
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
    index_name_with(table, columns, "idx")
}

fn index_name_with(table: &str, columns: &[String], suffix: &str) -> String {
    let full = format!("{}_{}_{}", table, columns.join("_"), suffix);
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
/// Schema changes declared on a `psql` infra beyond its entity's fields.
#[derive(Default)]
pub struct TableOptions<'a> {
    /// `unique: [...]` keys, each a `CREATE UNIQUE INDEX`.
    pub unique: &'a [Vec<String>],
    /// `renamed { old: new }` — applied before new columns are added, so
    /// the data follows the rename instead of a fresh empty column.
    pub renamed: &'a [(String, String)],
    /// `dropped: [...]` columns.
    pub dropped: &'a [String],
}

#[derive(Serialize)]
struct RenameView {
    old: String,
    new: String,
}

#[derive(Serialize)]
struct ForeignKeyView {
    name: String,
    column: String,
    ref_table: String,
    on_delete: String,
}

/// One foreign key of a psql entity, resolved by the generator.
pub struct ForeignKeySpec {
    pub column: String,
    pub ref_table: String,
    /// `cascade` / `set_null` / `restrict`, or `None` for the default.
    pub on_delete: Option<String>,
}

/// `migrations/<table>.fk.sql`: one idempotent `ADD CONSTRAINT ... FOREIGN
/// KEY` per reference. Run after every table of the app exists.
pub fn render_foreign_keys(entity: &RustType, keys: &[ForeignKeySpec]) -> Result<String, String> {
    let table = table_name(&entity.name);
    let mut views = Vec::new();
    for key in keys {
        let field = entity
            .fields
            .iter()
            .find(|f| f.name == key.column)
            .ok_or_else(|| format!("psql entity '{}': 'references' names unknown field '{}'", entity.name, key.column))?;
        if field.field_type.unwrap_option().name != "i64" {
            return Err(format!(
                "psql entity '{}': 'references' field '{}' must be Int or Option<Int> (it holds another row's id)",
                entity.name, key.column
            ));
        }
        let on_delete = match key.on_delete.as_deref() {
            Some("cascade") => "CASCADE",
            Some("restrict") => "RESTRICT",
            Some("set_null") if !field.field_type.is_option() => {
                return Err(format!(
                    "psql entity '{}': '{}' can't use set_null — the column is not nullable (make it Option<Int>)",
                    entity.name, key.column
                ));
            }
            Some(_) => "SET NULL",
            None if field.field_type.is_option() => "SET NULL",
            None => "RESTRICT",
        };
        views.push(ForeignKeyView {
            name: constraint_name(&table, &key.column, "fkey"),
            column: key.column.clone(),
            ref_table: key.ref_table.clone(),
            on_delete: on_delete.to_string(),
        });
    }
    let mut context = Context::new();
    context.insert("entity", &entity.name);
    context.insert("table", &table);
    context.insert("keys", &views);
    Ok(render("migration/foreign_keys.sql.tera", &context) + "\n")
}

pub fn foreign_keys_file_name(entity_name: &str) -> String {
    format!("{}.fk.sql", table_name(entity_name))
}

fn constraint_name(table: &str, column: &str, suffix: &str) -> String {
    let full = format!("{}_{}_{}", table, column, suffix);
    if full.len() <= 63 {
        return full;
    }
    let hash = full
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("{}_{:08x}", &full[..50], hash as u32)
}

pub fn render_create_table(entity: &RustType, indexes: &[Vec<String>]) -> Result<String, String> {
    render_table(entity, indexes, &TableOptions::default())
}

pub fn render_table(entity: &RustType, indexes: &[Vec<String>], options: &TableOptions) -> Result<String, String> {
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

    let has_field = |name: &str| entity.fields.iter().any(|f| f.name == name);
    let mut unique_views: Vec<IndexView> = Vec::new();
    for key in options.unique {
        for col in key {
            if !has_field(col) {
                return Err(format!("psql entity '{}': 'unique' names unknown field '{}'", entity.name, col));
            }
        }
        unique_views.push(IndexView {
            name: index_name_with(&table, key, "key"),
            columns: key.join(", "),
        });
    }
    let mut renames = Vec::new();
    for (old, new) in options.renamed {
        if !has_field(new) {
            return Err(format!(
                "psql entity '{}': 'renamed' {} -> {}, but '{}' is not a field of the entity",
                entity.name, old, new, new
            ));
        }
        if has_field(old) {
            return Err(format!(
                "psql entity '{}': 'renamed' {} -> {}, but '{}' is still a field of the entity",
                entity.name, old, new, old
            ));
        }
        renames.push(RenameView { old: old.clone(), new: new.clone() });
    }
    for col in options.dropped {
        if has_field(col) {
            return Err(format!(
                "psql entity '{}': 'dropped' column '{}' is still a field of the entity",
                entity.name, col
            ));
        }
    }

    let mut context = Context::new();
    context.insert("entity", &entity.name);
    context.insert("table", &table);
    context.insert("columns", &columns);
    context.insert("indexes", &index_views);
    context.insert("uniques", &unique_views);
    context.insert("renames", &renames);
    context.insert("dropped", options.dropped);
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
