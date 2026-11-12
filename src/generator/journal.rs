//! `journal {}` code: the crate's `trust_journal.rs` (writing entries,
//! undo / redo).

use std::path::Path;

use heck::ToSnakeCase;
use serde::Serialize;
use tera::Context;

use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::{file::psql_storage_file::insert_field_views, imports::DATE_TIME, migration, render::render},
        types::JSON_VALUE,
    },
    parser::AllowedType,
};

/// Fields a journal entity must have (besides its owner), and their type.
const REQUIRED: &[(&str, &str)] = &[
    ("id", "i64"),
    ("kind", "String"),
    ("text", "String"),
    ("changes", JSON_VALUE),
    ("undone", "bool"),
    ("at", DATE_TIME),
];

#[derive(Serialize)]
struct JournalView {
    fn_name: String,
    entity: String,
    table: String,
    owner: String,
    overwrite: bool,
    fields: Vec<serde_json::Value>,
    extra_fields: Vec<String>,
    all_columns: String,
}

#[derive(Serialize)]
struct TableView {
    table: String,
    columns: String,
}

/// How a `journal:` use case records its runs (rendered by the use case
/// impl template).
#[derive(Debug, Clone, Serialize)]
pub struct JournalUse {
    pub name: String,
    pub fn_name: String,
    /// The text template as a Rust string literal (wire field names).
    pub text_literal: String,
    /// The template reads `{input...}`.
    pub input_json: bool,
    /// The output's `action_id` field: `option`, `int` or empty (none).
    pub action_id: String,
    /// A `code {}` use case: its struct holds the `trust_journal` handle.
    pub handle: bool,
}

/// `{input.title}` / `{output.task.title}` with wire field names.
fn wire_template(template: &str, camel: bool) -> String {
    let re = regex::Regex::new(r"\{(input|output)((?:\.[A-Za-z0-9_]+)+)\}").expect("valid regex");
    re.replace_all(template, |caps: &regex::Captures| {
        let path: Vec<String> = caps[2]
            .trim_start_matches('.')
            .split('.')
            .map(|part| crate::generator::schema::wire_name(part, camel))
            .collect();
        format!("{{{}.{}}}", &caps[1], path.join("."))
    })
    .to_string()
}

impl CodeGen {
    /// The journaling of `use_case`, if it names a journal.
    pub(super) fn journal_use(
        context: &CodeGenContext,
        use_case: &crate::parser::UseCaseService,
        output: Option<&crate::generator::rust_code::types::RustType>,
        handle: bool,
    ) -> Result<Option<JournalUse>, String> {
        let Some(name) = &use_case.journal else { return Ok(None) };
        let journal = context
            .find_journal(name)
            .ok_or_else(|| format!("UseCase '{}': unknown journal '{}'", use_case.name, name))?;
        let template = use_case.journal_text.clone().unwrap_or_else(|| use_case.name.clone());
        let wired = wire_template(&template, context.json_camel());
        let action_id = output
            .and_then(|o| o.fields.iter().find(|f| f.name == "action_id"))
            .map(|f| match (f.field_type.is_option(), f.field_type.unwrap_option().name.as_str()) {
                (true, "i64") => Ok("option".to_string()),
                (false, "i64") => Ok("int".to_string()),
                _ => Err(format!("UseCase '{}': output field 'action_id' must be Int or Option<Int>", use_case.name)),
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Some(JournalUse {
            name: journal.name.clone(),
            fn_name: journal.name.to_snake_case(),
            text_literal: format!("{:?}", wired),
            input_json: wired.contains("{input."),
            action_id,
            handle,
        }))
    }

    /// Writes `src/trust_journal.rs` when the crate stores a journal;
    /// returns whether it did.
    pub(super) fn write_journal_module(&self, context: &mut CodeGenContext, src_path: &Path) -> Result<bool, String> {
        let mut journals = Vec::new();
        for name in context.journal_names() {
            let journal = context.find_journal(&name).cloned().ok_or("journal vanished")?;
            let Some(AllowedType::DefinedType(entity_name)) = context.find_infra_entity(&journal.infra).cloned() else {
                continue;
            };
            if context.find_infra_target_address(&journal.infra).is_some() {
                return Err(format!("journal '{}': infra '{}' must be psql", name, journal.infra));
            }
            let entity = context
                .find_type(&entity_name)
                .cloned()
                .ok_or_else(|| format!("journal '{}': unknown entity '{}'", name, entity_name))?;
            let mut required: Vec<(&str, &str)> = REQUIRED.to_vec();
            required.push((journal.owner.as_str(), "i64"));
            for (field, rust_type) in &required {
                match entity.fields.iter().find(|f| f.name == *field) {
                    Some(f) if f.field_type.name == *rust_type => {}
                    _ => {
                        return Err(format!(
                            "journal '{}': entity '{}' needs a field '{}' of type {}",
                            name,
                            entity_name,
                            field,
                            match *rust_type {
                                "i64" => "Int",
                                "bool" => "Bool",
                                JSON_VALUE => "Json",
                                DATE_TIME => "DateTime",
                                other => other,
                            }
                        ));
                    }
                }
            }
            let extra_fields = entity
                .fields
                .iter()
                .filter(|f| !required.iter().any(|(r, _)| *r == f.name))
                .map(|f| f.name.clone())
                .collect();
            journals.push(JournalView {
                fn_name: name.to_snake_case(),
                entity: entity_name.clone(),
                table: migration::table_name(&entity_name),
                owner: journal.owner.clone(),
                overwrite: journal.overwrite,
                fields: insert_field_views(&entity)?,
                extra_fields,
                all_columns: entity.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", "),
            });
        }
        if journals.is_empty() {
            return Ok(false);
        }

        // Every psql table of the crate can be restored by an undo.
        let mut tables = Vec::new();
        for entity_name in context.psql_entity_names() {
            let Some(entity) = context.find_type(&entity_name) else { continue };
            if !entity.fields.iter().any(|f| f.name == "id") {
                continue;
            }
            let table = migration::table_name(&entity_name);
            if tables.iter().any(|t: &TableView| t.table == table) {
                continue;
            }
            tables.push(TableView {
                table,
                columns: entity.fields.iter().filter(|f| f.name != "id").map(|f| f.name.clone()).collect::<Vec<_>>().join(", "),
            });
        }

        let mut ctx = Context::new();
        ctx.insert("journals", &journals);
        ctx.insert("tables", &tables);
        std::fs::write(src_path.join("trust_journal.rs"), render("file/journal/journal.rs.tera", &ctx))
            .map_err(|e| format!("Failed to write the journal module: {}", e))?;
        context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
        Ok(true)
    }
}
