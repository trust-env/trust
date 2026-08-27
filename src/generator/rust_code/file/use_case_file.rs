use std::{fs::File, io::Write, path::PathBuf};

use heck::ToSnakeCase;

use crate::{
    generator::rust_code::{
        file::psql_storage_file::STORAGE_ERROR_VARIANTS,
        imports::{LocalImports, common_use},
        render::render,
        types::{RustField, RustType},
    },
    parser::{ErrorMapping, FilterMapping},
};
use serde::Serialize;
use tera::Context;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    Create,
    Read,
    Update,
    Delete,
    List,
}

impl CallKind {
    pub fn from_method(method: &str) -> Result<Self, String> {
        match method {
            "save_one" => Ok(CallKind::Create),
            "find_one" => Ok(CallKind::Read),
            "update_one" => Ok(CallKind::Update),
            "delete" => Ok(CallKind::Delete),
            "list" => Ok(CallKind::List),
            other => Err(format!(
                "Unsupported call method '{}': expected one of save_one, find_one, update_one, delete, list",
                other
            )),
        }
    }

    /// Fields the use case's input struct must declare for this call shape,
    /// beyond whatever fields it needs for entity reconstruction.
    pub fn required_input_fields(&self) -> &'static [&'static str] {
        match self {
            CallKind::Create => &[],
            CallKind::Read | CallKind::Delete | CallKind::Update => &["id"],
            CallKind::List => &["limit", "offset"],
        }
    }
}

/// One resolved `call: Target.method` line. A use case can declare more than
/// one — the last one is primary (drives the output shape); earlier ones run
/// first as side effects/guards sharing the same input. Each call carries
/// its own `errors {}` mapping (from the `call: { name: ..., errors {...} }`
/// block form), independent of any other call the same use case makes.
#[derive(Clone)]
pub struct ResolvedCall {
    pub infra_name: String,
    pub entity_type: RustType,
    pub storage_trait: String,
    pub storage_error: String,
    pub method: String,
    pub call_kind: CallKind,
    pub errors: Vec<ErrorMapping>,
}

pub struct UseCaseFile {
    name: String,
    input: RustType,
    /// In declaration order; the last entry is primary (determines
    /// output/error `NotFound` semantics and the `{Output}` shape).
    calls: Vec<ResolvedCall>,
    /// True when `input` was inferred as a direct copy of the primary call's
    /// entity (no explicit request type declared) — in that case the
    /// entity's own `id` is part of the guessed input too, so the identity
    /// transform carries it straight through instead of defaulting it. Only
    /// ever true when there's exactly one call (guessing requires an
    /// unambiguous primary).
    copy_id: bool,
    /// Non-empty only when the primary call is `list` and a `filters {}`
    /// block was declared — each becomes one equality `StoragePredicate` in
    /// the generated `StorageListQuery`. See `FilterMapping`.
    filters: Vec<FilterMapping>,
    file: File,
}

impl UseCaseFile {
    pub fn new(
        name: String,
        input: RustType,
        calls: Vec<ResolvedCall>,
        copy_id: bool,
        filters: Vec<FilterMapping>,
        folder: PathBuf,
    ) -> Result<Self, String> {
        let file_path = folder.join(format!("{}.rs", name.to_snake_case()));

        Ok(UseCaseFile {
            file: File::create(&file_path)
                .map_err(|e| format!("Failed to create use case file: {}", e))?,
            name,
            input,
            calls,
            copy_id,
            filters,
        })
    }

    fn primary(&self) -> &ResolvedCall {
        self.calls.last().expect("UseCaseFile always has at least one call")
    }

    fn input_name(&self) -> String {
        format!("{}Input", self.name)
    }

    fn output_name(&self) -> String {
        format!("{}Output", self.name)
    }

    fn error_name(&self) -> String {
        format!("{}Error", self.name)
    }

    fn trait_name(&self) -> String {
        format!("{}Trait", self.name)
    }

    fn field_name(call: &ResolvedCall) -> String {
        call.infra_name.to_snake_case()
    }

    /// Calls deduped by infra target — each distinct infra gets exactly one
    /// storage field/generic param on the use case struct, even if it's
    /// called more than once (e.g. a check then a write on the same infra).
    fn unique_calls(&self) -> Vec<&ResolvedCall> {
        let mut seen = Vec::new();
        let mut result = Vec::new();

        for call in &self.calls {
            if !seen.contains(&call.infra_name) {
                seen.push(call.infra_name.clone());
                result.push(call);
            }
        }
        result
    }

    fn generics(&self) -> String {
        (0..self.unique_calls().len())
            .map(|i| format!("S{}", i))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn where_clause(&self) -> String {
        self.unique_calls()
            .iter()
            .enumerate()
            .map(|(i, call)| format!("    S{}: {},\n", i, call.storage_trait))
            .collect()
    }

    /// The `id`/`limit`/`offset` style fields on an explicit request type come
    /// from the request's own lookup/paging concerns, not the entity's own
    /// fields, so `id` is left at its default unless `copy_id` says the input
    /// is a guessed direct copy of the entity (where `id` really is the same
    /// field).
    fn field_init(&self, field: &RustField, copy_id: bool) -> String {
        if (field.name != "id" || copy_id) && self.input.fields.iter().any(|f| f.name == field.name) {
            format!("{}: input.{},\n", field.name, field.name)
        } else {
            format!("{}: Default::default(),\n", field.name)
        }
    }

    fn gen_input_struct(&self) -> String {
        #[derive(Serialize)]
        struct FieldView {
            name: String,
            field_type: String,
        }

        let fields: Vec<FieldView> = self
            .input
            .fields
            .iter()
            .map(|field| FieldView {
                name: field.name.clone(),
                field_type: field.field_type.to_call_string(Vec::new()),
            })
            .collect();

        let mut context = Context::new();
        context.insert("name", &self.input_name());
        context.insert("fields", &fields);
        render("file/use_case/input_struct.rs.tera", &context)
    }

    fn gen_entity_conversion(&self, entity: &RustType, copy_id: bool) -> String {
        let entity_type = entity.to_call_string(Vec::new());
        // `field_init` already returns "name: value,\n" — strip the trailing
        // newline the template's own loop adds back per line.
        let field_lines: Vec<String> = entity
            .fields
            .iter()
            .map(|field| self.field_init(field, copy_id).trim_end().to_string())
            .collect();

        let mut context = Context::new();
        context.insert("input_name", &self.input_name());
        context.insert("entity_type", &entity_type);
        context.insert("field_lines", &field_lines);
        render("file/use_case/entity_conversion.rs.tera", &context)
    }

    /// Entities needing a `From<Input> for Entity` impl, deduped by entity
    /// name (only Create/Update calls need one). `copy_id` only ever applies
    /// to the primary call's entity, and only when it was itself derived
    /// from a guessed (not explicit) input.
    fn entities_needing_conversion(&self) -> Vec<(&RustType, bool)> {
        let mut seen = Vec::new();
        let mut result = Vec::new();
        let last_index = self.calls.len() - 1;

        for (i, call) in self.calls.iter().enumerate() {
            if !matches!(call.call_kind, CallKind::Create | CallKind::Update) {
                continue;
            }
            if seen.contains(&call.entity_type.name) {
                continue;
            }
            seen.push(call.entity_type.name.clone());
            result.push((&call.entity_type, i == last_index && self.copy_id));
        }
        result
    }

    fn gen_output_struct(&self) -> String {
        let primary = self.primary();
        let entity_type = primary.entity_type.to_call_string(Vec::new());
        let is_list = primary.call_kind == CallKind::List;
        let field_name = if is_list {
            format!("{}s", primary.entity_type.name.to_snake_case())
        } else {
            primary.entity_type.name.to_snake_case()
        };

        let mut context = Context::new();
        context.insert("name", &self.output_name());
        context.insert("field", &field_name);
        context.insert("entity_type", &entity_type);
        context.insert("is_list", &is_list);
        render("file/use_case/output_struct.rs.tera", &context)
    }

    fn mapped_error<'a>(errors: &'a [ErrorMapping], source: &str) -> Option<&'a ErrorMapping> {
        errors.iter().find(|m| m.source == source)
    }

    /// Every custom error target across every call, deduped — the enum needs
    /// exactly one variant per unique target name regardless of which call
    /// produces it.
    fn unique_error_targets(&self) -> Vec<&str> {
        let mut seen = Vec::new();

        for call in &self.calls {
            for mapping in &call.errors {
                if !seen.contains(&mapping.target.as_str()) {
                    seen.push(mapping.target.as_str());
                }
            }
        }
        seen
    }

    /// Storage error types referenced by any call, deduped — each gets its
    /// own `From` impl converting into the same shared `StorageError(String)`
    /// / custom-target variants below, using that call's own `errors {}`
    /// mapping (the first call using a given storage error type wins if more
    /// than one call shares it).
    fn unique_storage_errors(&self) -> Vec<&ResolvedCall> {
        let mut seen = Vec::new();
        let mut result = Vec::new();

        for call in &self.calls {
            if !seen.contains(&call.storage_error) {
                seen.push(call.storage_error.clone());
                result.push(call);
            }
        }
        result
    }

    /// The use case's error enum is shared across every infra it calls: the
    /// fallback `StorageError` variant holds a formatted `String` (rather
    /// than one specific storage crate's error type) precisely so it can
    /// absorb failures from any number of distinct storage types.
    fn gen_error(&self) -> String {
        let name = self.error_name();

        #[derive(Serialize)]
        struct StorageImplView {
            storage_error: String,
            has_custom_errors: bool,
            match_arms: Vec<String>,
            has_fallback: bool,
        }

        let storage_impls: Vec<StorageImplView> = self
            .unique_storage_errors()
            .into_iter()
            .map(|call| {
                let storage_error = call.storage_error.clone();

                if call.errors.is_empty() {
                    return StorageImplView {
                        storage_error,
                        has_custom_errors: false,
                        match_arms: Vec::new(),
                        has_fallback: false,
                    };
                }

                let all_covered = STORAGE_ERROR_VARIANTS
                    .iter()
                    .all(|v| Self::mapped_error(&call.errors, v).is_some());

                let match_arms: Vec<String> = STORAGE_ERROR_VARIANTS
                    .iter()
                    .filter_map(|variant| {
                        let mapping = Self::mapped_error(&call.errors, variant)?;
                        let (binding, value_expr) = match &mapping.message {
                            Some(m) => ("_msg".to_string(), format!("{:?}.to_string()", m)),
                            None => ("msg".to_string(), "msg".to_string()),
                        };
                        Some(format!(
                            "{storage_error}::{variant}({binding}) => {name}::{target}({value_expr})",
                            storage_error = storage_error,
                            variant = variant,
                            binding = binding,
                            name = name,
                            target = mapping.target,
                            value_expr = value_expr,
                        ))
                    })
                    .collect();

                StorageImplView {
                    storage_error,
                    has_custom_errors: true,
                    match_arms,
                    has_fallback: !all_covered,
                }
            })
            .collect();

        let mut context = Context::new();
        context.insert("name", &name);
        context.insert("error_targets", &self.unique_error_targets());
        context.insert("storage_impls", &storage_impls);
        render("file/use_case/error.rs.tera", &context)
    }

    fn gen_trait(&self) -> String {
        let mut context = Context::new();
        context.insert("trait_name", &self.trait_name());
        context.insert("input", &self.input_name());
        context.insert("output", &self.output_name());
        context.insert("error", &self.error_name());
        // Shares the exact same shape as the ai-use-case trait declaration.
        render("file/ai_use_case/trait.rs.tera", &context)
    }

    fn gen_struct(&self) -> String {
        #[derive(Serialize)]
        struct StructFieldView {
            field: String,
            index: usize,
        }

        let struct_fields: Vec<StructFieldView> = self
            .unique_calls()
            .iter()
            .enumerate()
            .map(|(i, call)| StructFieldView {
                field: Self::field_name(call),
                index: i,
            })
            .collect();
        let params = self
            .unique_calls()
            .iter()
            .enumerate()
            .map(|(i, call)| format!("{}: S{}", Self::field_name(call), i))
            .collect::<Vec<_>>()
            .join(", ");
        let ctor_fields: Vec<String> = self.unique_calls().iter().map(|call| Self::field_name(call)).collect();

        let mut context = Context::new();
        context.insert("name", &self.name);
        context.insert("generics", &self.generics());
        context.insert("where_clause", &self.where_clause());
        context.insert("struct_fields", &struct_fields);
        context.insert("params", &params);
        context.insert("ctor_fields", &ctor_fields);
        render("file/use_case/struct.rs.tera", &context)
    }

    /// Builds the `StorageListQuery { ... }` expression a `list` call binds
    /// to — `predicates` is empty unless `filters {}` was declared, in
    /// which case each entry becomes one equality `StoragePredicate`
    /// comparing the entity field against the matching input field
    /// (`.to_string()`'d, same conversion `auth {}`'s own permission-row
    /// lookup already uses for its own predicates).
    fn list_query_expr(&self) -> String {
        if self.filters.is_empty() {
            return "StorageListQuery {\n                limit: input.limit,\n                offset: input.offset,\n                predicates: vec![],\n            }"
                .to_string();
        }

        let predicates: Vec<String> = self
            .filters
            .iter()
            .map(|f| {
                format!(
                    "StoragePredicate {{ field: {:?}.to_string(), operator: StoragePredicateOperator::Eq, value: input.{}.to_string() }}",
                    f.field, f.input_field
                )
            })
            .collect();

        format!(
            "StorageListQuery {{\n                limit: input.limit,\n                offset: input.offset,\n                predicates: vec![\n                    {}\n                ],\n            }}",
            predicates.join(",\n                    ")
        )
    }

    /// Renders one call's expression: `is_tail` (the primary, always the
    /// last declared call) produces the function's tail expression (no
    /// trailing `;`); every earlier call produces a statement that clones
    /// whatever it reads from `input` (since later calls still need it) and
    /// propagates its error with `?`, discarding the `Ok` value.
    fn gen_call_expr(&self, call: &ResolvedCall, is_tail: bool) -> String {
        let output = self.output_name();
        let error = self.error_name();
        let entity = call.entity_type.to_call_string(Vec::new());
        let field = Self::field_name(call);

        let id_expr = || if is_tail { "input.id".to_string() } else { "input.id.clone()".to_string() };
        let entity_expr = || {
            if is_tail {
                format!("{}::from(input)", entity)
            } else {
                format!("{}::from(input.clone())", entity)
            }
        };

        let args = match call.call_kind {
            CallKind::Create => entity_expr(),
            CallKind::Read | CallKind::Delete => id_expr(),
            CallKind::Update => format!("{}, {}", id_expr(), entity_expr()),
            CallKind::List => self.list_query_expr(),
        };

        let mut context = Context::new();
        context.insert("field", &field);
        context.insert("method", &call.method);
        context.insert("args", &args);
        context.insert("output", &output);
        context.insert("error", &error);
        context.insert("is_tail", &is_tail);

        match call.call_kind {
            CallKind::Read => render("file/use_case/call_read.rs.tera", &context),
            _ => render("file/use_case/call_simple.rs.tera", &context),
        }
    }

    fn gen_execute_body(&self) -> String {
        let mut code = String::new();
        let last_index = self.calls.len() - 1;

        for (i, call) in self.calls.iter().enumerate() {
            let is_tail = i == last_index;
            code.push_str(&self.gen_call_expr(call, is_tail));
            if !is_tail {
                code.push('\n');
            }
        }
        code
    }

    fn gen_impl(&self) -> String {
        let mut context = Context::new();
        context.insert("generics", &format!("<{}>", self.generics()));
        context.insert("input", &self.input_name());
        context.insert("output", &self.output_name());
        context.insert("error", &self.error_name());
        context.insert("name", &self.name);
        context.insert("where_clause", &format!("where\n{}", self.where_clause()));
        context.insert("body", &self.gen_execute_body());
        context.insert("trait_name", &self.trait_name());
        // Shares the exact same shape as the ai-use-case impl block.
        render("file/ai_use_case/impl.rs.tera", &context)
    }

    fn gen_imports(&self) -> String {
        let mut imports = LocalImports::from_fields(&self.input.fields, &self.input_name());

        for call in &self.calls {
            imports.add(&call.entity_type, "");
            if !imports.types.contains(&call.storage_trait) {
                imports.types.push(call.storage_trait.clone());
            }
            if !imports.types.contains(&call.storage_error) {
                imports.types.push(call.storage_error.clone());
            }
        }

        let mut common_items = vec!["UseCase"];
        if self.calls.iter().any(|c| c.call_kind == CallKind::List) {
            common_items.push("StorageListQuery");
        }
        if !self.filters.is_empty() {
            common_items.push("StoragePredicate");
            common_items.push("StoragePredicateOperator");
        }

        let mut code = common_use(&common_items);
        code.push_str(&imports.to_use_statements());
        code
    }

    pub fn write(&mut self) -> Result<(), String> {
        let mut code = String::new();

        code.push_str(&self.gen_imports());
        code.push_str(&self.gen_input_struct());
        for (entity, copy_id) in self.entities_needing_conversion() {
            code.push_str(&self.gen_entity_conversion(entity, copy_id));
        }
        code.push_str(&self.gen_output_struct());
        code.push_str(&self.gen_error());
        code.push_str(&self.gen_trait());
        code.push_str(&self.gen_struct());
        code.push_str(&self.gen_impl());
        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to use case file: {}", e))
    }
}
