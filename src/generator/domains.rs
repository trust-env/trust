use std::path::{Path, PathBuf};

use heck::ToSnakeCase;
use serde::Serialize;
use tera::Context;

use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::{
            imports::LocalImports,
            render::render,
            types::{RustType, TypeKind},
        },
        types::enum_variant_ident,
    },
    parser::TypeDeclaration,
};

#[derive(Serialize)]
struct FieldView {
    name: String,
    field_type: String,
    /// Name of the generated `crate::validate_regex_N` function checking
    /// this field, when it is a `Regex<...>` (or `Option<Regex<...>>`).
    regex_fn: Option<String>,
    regex_optional: bool,
    /// `plain` / `nullable` when the field is stored as `JSONB` through
    /// `sqlx::types::Json` (lists, nested structs); empty otherwise.
    sqlx_json: &'static str,
}

#[derive(Serialize)]
struct VariantView {
    ident: String,
    wire: String,
}

#[derive(Serialize)]
struct RegexView {
    fn_name: String,
    /// The pattern as a Rust string literal.
    pattern_literal: String,
}

impl CodeGen {
    pub(super) fn gen_domain(
        &mut self,
        context: &mut CodeGenContext,
        t_decl: &TypeDeclaration,
        folder: PathBuf,
    ) -> Result<(), String> {
        let t = self.resolve_type_declaration(context, t_decl)?;
        write_domain_type(context, t, &folder)
    }
}

/// Field views for a struct's fields, registering every `Regex<...>`
/// pattern they use with the context (rendered later into the crate's
/// shared validator module).
pub(super) fn field_views(
    context: &mut CodeGenContext,
    t: &RustType,
    is_storage_entity: bool,
) -> Vec<serde_json::Value> {
    t.fields
        .iter()
        .map(|field| {
            let regex_fn = field
                .field_type
                .regex_pattern()
                .map(|p| context.register_regex(p));
            let sqlx_json = match (is_storage_entity, field.field_type.is_json_wrapped()) {
                (true, true) if field.field_type.is_option() => "nullable",
                (true, true) => "plain",
                _ => "",
            };
            serde_json::to_value(FieldView {
                name: field.name.clone(),
                field_type: field.field_type.to_call_string(Vec::new()),
                regex_optional: field.field_type.is_option(),
                regex_fn,
                sqlx_json,
            })
            .expect("field view serializes")
        })
        .collect()
}

/// The validator function generated for a `Regex<...>` pattern — named
/// after a hash of the pattern, so any generated file can reference it
/// without coordinating indexes.
pub fn regex_fn_name(pattern: &str) -> String {
    let hash = pattern
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("validate_regex_{:016x}", hash)
}

/// Registers the crate dependencies a type's fields need.
pub(super) fn require_field_dependencies(
    context: &mut CodeGenContext,
    imports: &LocalImports,
    is_storage_entity: bool,
    has_regex: bool,
) {
    if imports.needs_uuid {
        context.require_version_dependency_with_features("uuid", "1", vec!["serde"]);
    }
    if imports.needs_decimal {
        // "serde-str" forces Decimal to (de)serialize as a JSON string
        // rather than a float, so money/precision fields never round-trip
        // through IEEE-754 float imprecision over the wire.
        context.require_version_dependency_with_features("rust_decimal", "1", vec!["serde-str"]);
    }
    if imports.needs_chrono {
        context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
    }
    if imports.needs_json {
        context.require_version_dependency("serde_json", "1");
    }
    if has_regex {
        context.require_version_dependency("regex", "1");
    }
    context.require_version_dependency_with_features("serde", "1", vec!["derive"]);
    if is_storage_entity {
        let mut features = vec!["runtime-tokio", "tls-rustls", "postgres"];
        if imports.needs_uuid {
            features.push("uuid");
        }
        if imports.needs_decimal {
            features.push("rust_decimal");
        }
        if imports.needs_chrono {
            features.push("chrono");
        }
        if imports.needs_json || imports.needs_json_column {
            features.push("json");
        }
        context.require_version_dependency_with_features("sqlx", "0.9.0", features);
    }
}

/// Writes `src/domains/<type>.rs` for one resolved declared type (struct,
/// enum or alias) and registers it with the crate.
pub(super) fn write_domain_type(
    context: &mut CodeGenContext,
    t: RustType,
    folder: &Path,
) -> Result<(), String> {
    context.add_type(t.clone());
    context.register_domain_module(t.to_file_name());

    let code = match &t.kind {
        TypeKind::Enum(variants) => {
            context.require_version_dependency_with_features("serde", "1", vec!["derive"]);
            let variants: Vec<VariantView> = variants
                .iter()
                .map(|wire| VariantView {
                    ident: enum_variant_ident(wire),
                    wire: wire.clone(),
                })
                .collect();
            let mut ctx = Context::new();
            ctx.insert("name", &t.name);
            ctx.insert("variants", &variants);
            // Column encoding is only needed (and `sqlx` only present)
            // when the crate talks to Postgres.
            ctx.insert("sqlx", &context.has_psql());
            render("domains/enum.rs.tera", &ctx)
        }
        _ => {
            let imports = LocalImports::from_fields(&t.fields, &t.name);
            let is_storage_entity = context.is_storage_entity(&t.name);
            let fields = field_views(context, &t, is_storage_entity);
            let has_regex = t.fields.iter().any(|f| f.field_type.regex_pattern().is_some());
            require_field_dependencies(context, &imports, is_storage_entity, has_regex);

            let mut ctx = Context::new();
            ctx.insert("is_storage_entity", &is_storage_entity);
            ctx.insert("is_public", &true);
            ctx.insert("name", &t.name);
            ctx.insert("fields", &fields);
            imports.to_use_statements()
                + &render("domains/derive.rs.tera", &ctx)
                + &render("types.rs.tera", &ctx)
        }
    };

    let path = folder.join(format!("{}.rs", t.name.to_snake_case()));
    std::fs::write(&path, code).map_err(|e| format!("Failed to write domain file {}: {}", path.display(), e))
}

/// Writes `src/domains/trust_validation.rs` with one validator per
/// distinct `Regex<...>` pattern the crate uses (nothing when none).
pub(super) fn write_regex_validators(context: &mut CodeGenContext, folder: &Path) -> Result<(), String> {
    let patterns = context.regex_patterns().to_vec();
    if patterns.is_empty() && !context.needs_double_option() {
        return Ok(());
    }
    let regexes: Vec<RegexView> = patterns
        .iter()
        .map(|pattern| RegexView {
            fn_name: regex_fn_name(pattern),
            pattern_literal: format!("{:?}", pattern),
        })
        .collect();
    let mut ctx = Context::new();
    ctx.insert("regexes", &regexes);
    ctx.insert("double_option", &context.needs_double_option());
    std::fs::write(
        folder.join("trust_validation.rs"),
        render("domains/validation.rs.tera", &ctx),
    )
    .map_err(|e| format!("Failed to write regex validators: {}", e))?;
    context.register_domain_module("trust_validation".to_string());
    if !patterns.is_empty() {
        context.require_version_dependency("regex", "1");
    }
    Ok(())
}
