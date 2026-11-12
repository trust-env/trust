use std::{fs::File, io::Write, path::PathBuf};

use heck::ToSnakeCase;

use crate::{
    generator::rust_code::{
        file::psql_storage_file::STORAGE_ERROR_VARIANTS,
        fragments::{type_params, where_block, where_clause},
        imports::{DATE_TIME, LocalImports, common_use},
        render::render,
        types::{RustField, RustType},
    },
    parser::{ErrorMapping, FilterMapping, FilterOperator, FilterSource, OrderBy},
};
use serde::Serialize;
use tera::Context;

/// How one entity field is filled in a `From<Input> for Entity` impl.
#[derive(Serialize)]
struct FieldInitView {
    name: String,
    /// `input` (copied from the input), `now` (server-stamped
    /// `DateTime`) or `default`.
    source: &'static str,
}

/// One `StorageError::Variant(..) => UseCaseError::Target(..)` match arm.
#[derive(Serialize)]
struct ErrorArmView {
    variant: String,
    target: String,
    /// The mapping's fixed message as a Rust string literal; `None` keeps
    /// the storage error's own message.
    message_literal: Option<String>,
}

/// One predicate of a generated `StorageListQuery`.
#[derive(Serialize)]
struct PredicateView {
    /// The column name as a Rust string literal.
    field_literal: String,
    /// The `StoragePredicateOperator` variant.
    operator: &'static str,
    /// `input` (an input field), `literal` (a fixed value, already
    /// formatted), `user`, `now` or `today`.
    source: &'static str,
    /// The input field name, or the literal value as a Rust string literal.
    value: String,
    /// How an input value is turned into text: `display` (`to_string()`),
    /// `join` (a list, comma-joined for `in`).
    conversion: &'static str,
    /// The input field is an `Option`: the predicate only applies when set.
    optional: bool,
    /// `contains`: the value is wrapped in `%...%`.
    contains: bool,
}

#[derive(Serialize)]
struct PatchFieldView {
    name: String,
    /// `input`, `some` or `none`.
    source: &'static str,
}

#[derive(Serialize)]
struct OrderView {
    field: String,
    descending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallKind {
    Create,
    Read,
    Update,
    Delete,
    List,
    /// `upsert_one`: insert, or update the row with the same unique key.
    Upsert,
    /// `patch_one`: update only the fields the input carries.
    Patch,
    /// `save_or_patch`: create without `id`, patch with one.
    SaveOrPatch,
}

impl CallKind {
    pub fn from_method(method: &str) -> Result<Self, String> {
        match method {
            "save_one" => Ok(CallKind::Create),
            "find_one" => Ok(CallKind::Read),
            "update_one" => Ok(CallKind::Update),
            "delete" => Ok(CallKind::Delete),
            "list" => Ok(CallKind::List),
            "upsert_one" => Ok(CallKind::Upsert),
            "patch_one" => Ok(CallKind::Patch),
            "save_or_patch" => Ok(CallKind::SaveOrPatch),
            other => Err(format!(
                "Unsupported call method '{}': expected one of save_one, find_one, update_one, patch_one, upsert_one, save_or_patch, delete, list",
                other
            )),
        }
    }

    /// Fields the use case's input struct must declare for this call shape,
    /// beyond whatever fields it needs for entity reconstruction.
    pub fn required_input_fields(&self) -> &'static [&'static str] {
        match self {
            CallKind::Create | CallKind::Upsert => &[],
            CallKind::Read | CallKind::Delete | CallKind::Update | CallKind::Patch | CallKind::SaveOrPatch => &["id"],
            // `limit`/`offset` are optional: absent means the defaults.
            CallKind::List => &[],
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
    owner: Option<String>,
    order_by: Vec<OrderBy>,
    /// `transaction: true`: every call runs in one database transaction.
    transaction: bool,
    /// Announce successful runs to `events:` streams.
    publish: bool,
    journal: Option<crate::generator::journal::JournalUse>,
    /// `output:` keeping some of the entity's fields (and `action_id`).
    output_projection: Option<RustType>,
    /// Fields with a `defaults {}` entry, per infra.
    default_fields: std::collections::HashMap<String, Vec<String>>,
    /// `config { json_case: "camel" }`.
    json_camel: bool,
    file: File,
}

impl UseCaseFile {
    pub fn new(
        name: String,
        input: RustType,
        calls: Vec<ResolvedCall>,
        copy_id: bool,
        filters: Vec<FilterMapping>,
        owner: Option<String>,
        order_by: Vec<OrderBy>,
        transaction: bool,
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
            owner,
            order_by,
            transaction,
            publish: false,
            json_camel: false,
            journal: None,
            output_projection: None,
            default_fields: std::collections::HashMap::new(),
        })
    }

    /// The `defaults {}` fields of each infra the calls target.
    pub fn with_default_fields(mut self, default_fields: std::collections::HashMap<String, Vec<String>>) -> Self {
        self.default_fields = default_fields;
        self
    }

    /// A journaled run (always in a transaction).
    pub fn with_journal(mut self, journal: Option<crate::generator::journal::JournalUse>) -> Self {
        if journal.is_some() {
            self.transaction = true;
        }
        self.journal = journal;
        self
    }

    pub fn with_output_projection(mut self, projection: Option<RustType>) -> Self {
        self.output_projection = projection;
        self
    }

    pub fn with_publish(mut self, publish: bool) -> Self {
        self.publish = publish;
        self
    }

    pub fn with_json_camel(mut self, json_camel: bool) -> Self {
        self.json_camel = json_camel;
        self
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

    fn type_param_names(&self) -> Vec<String> {
        (0..self.unique_calls().len()).map(|i| format!("S{}", i)).collect()
    }

    fn generics(&self) -> String {
        self.type_param_names().join(", ")
    }

    fn bounds(&self) -> Vec<(String, Vec<String>)> {
        self.unique_calls()
            .iter()
            .enumerate()
            .map(|(i, call)| {
                let mut bounds = vec![call.storage_trait.clone()];
                if self.transaction {
                    bounds.push("TrustTransactional".to_string());
                }
                (format!("S{}", i), bounds)
            })
            .collect()
    }

    fn where_clause(&self) -> String {
        where_clause(&self.bounds())
    }

    /// The `id`/`limit`/`offset` style fields on an explicit request type come
    /// from the request's own lookup/paging concerns, not the entity's own
    /// fields, so `id` is left at its default unless `copy_id` says the input
    /// is a guessed direct copy of the entity (where `id` really is the same
    /// field).
    /// A `DateTime` entity field the input doesn't carry (e.g.
    /// `created_at`) is server-stamped with the current time rather than
    /// defaulted to the epoch.
    fn field_init(&self, field: &RustField, copy_id: bool) -> FieldInitView {
        let input_field = self.input.fields.iter().find(|f| f.name == field.name);
        let source = if (field.name != "id" || copy_id) && input_field.is_some() {
            // An optional input for a field with a default.
            match input_field {
                Some(i) if i.field_type.is_option() && field.field_type.name == DATE_TIME => "input_or_now",
                Some(i) if i.field_type.is_option() && !field.field_type.is_option() => "input_unwrap",
                Some(i) if i.field_type.is_option() && i.field_type.unwrap_option().is_option() => "input_flatten",
                _ => "input",
            }
        } else if field.field_type.name == DATE_TIME {
            "now"
        } else {
            "default"
        };
        FieldInitView { name: field.name.clone(), source }
    }

    fn gen_input_struct(&self) -> String {
        #[derive(Serialize)]
        struct FieldView {
            name: String,
            field_type: String,
            regex_fn: Option<String>,
            regex_optional: bool,
            double_option: bool,
        }

        let fields: Vec<FieldView> = self
            .input
            .fields
            .iter()
            .map(|field| FieldView {
                name: field.name.clone(),
                field_type: field.field_type.to_call_string(Vec::new()),
                regex_fn: field.field_type.validator_key().map(|k| crate::generator::domains::regex_fn_name(&k)),
                regex_optional: field.field_type.is_option(),
                double_option: field.field_type.is_option() && field.field_type.unwrap_option().is_option(),
            })
            .collect();

        let mut context = Context::new();
        context.insert("name", &self.input_name());
        context.insert("fields", &fields);
        context.insert("camel", &self.json_camel);
        render("file/use_case/input_struct.rs.tera", &context)
    }

    fn gen_entity_conversion(&self, entity: &RustType, copy_id: bool) -> String {
        let entity_type = entity.to_call_string(Vec::new());
        let fields: Vec<FieldInitView> = entity
            .fields
            .iter()
            .map(|field| self.field_init(field, copy_id))
            .collect();

        let mut context = Context::new();
        context.insert("input_name", &self.input_name());
        context.insert("entity_type", &entity_type);
        context.insert("fields", &fields);
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
            if !matches!(call.call_kind, CallKind::Create | CallKind::Update | CallKind::Upsert) {
                continue;
            }
            if seen.contains(&call.entity_type.name) {
                continue;
            }
            seen.push(call.entity_type.name.clone());
            result.push((&call.entity_type, i == last_index && self.copy_id && call.call_kind != CallKind::Upsert));
        }
        result
    }

    fn gen_output_struct(&self) -> String {
        let primary = self.primary();
        if let Some(projection) = &self.output_projection {
            let entity = primary.entity_type.to_call_string(Vec::new());
            let mut code = String::from("#[derive(Debug, Clone, serde::Serialize)]\n");
            if self.json_camel {
                code.push_str("#[serde(rename_all = \"camelCase\")]\n");
            }
            code.push_str(&format!("/// `{}`, built from the {}.\npub struct {} {{\n", projection.name, entity, self.output_name()));
            for f in &projection.fields {
                code.push_str(&format!("    pub {}: {},\n", f.name, f.field_type.to_call_string(Vec::new())));
            }
            code.push_str(&format!("}}\n\nimpl From<{}> for {} {{\n    fn from(entity: {}) -> Self {{\n        Self {{\n", entity, self.output_name(), entity));
            for f in &projection.fields {
                if primary.entity_type.fields.iter().any(|e| e.name == f.name) {
                    code.push_str(&format!("            {}: entity.{},\n", f.name, f.name));
                } else {
                    code.push_str(&format!("            {}: Default::default(),\n", f.name));
                }
            }
            code.push_str("        }\n    }\n}\n\n");
            return code;
        }
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
        context.insert("camel", &self.json_camel);
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
        // `save_or_patch` creating without a required field.
        if self.calls.iter().any(|c| c.call_kind == CallKind::SaveOrPatch) {
            seen.push("MissingField");
        }

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
            match_arms: Vec<ErrorArmView>,
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

                let match_arms: Vec<ErrorArmView> = STORAGE_ERROR_VARIANTS
                    .iter()
                    .filter_map(|variant| {
                        let mapping = Self::mapped_error(&call.errors, variant)?;
                        Some(ErrorArmView {
                            variant: variant.to_string(),
                            target: mapping.target.clone(),
                            // Rust string literal of the fixed message, if any.
                            message_literal: mapping.message.as_ref().map(|m| format!("{:?}", m)),
                        })
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
    /// to: one `StoragePredicate` per `filters {}` line (plus the owner),
    /// values sent as text (`common`'s predicates are strings) and cast to
    /// the column type by the storage. An `Option` input field only filters
    /// when it is set. `limit`/`offset` fall back to defaults when the
    /// input doesn't carry them (or leaves them empty).
    fn list_query_expr(&self) -> String {
        let input_field = |name: &str| self.input.fields.iter().find(|f| f.name == name);
        let mut predicates: Vec<PredicateView> = self
            .filters
            .iter()
            .map(|f| {
                let field_literal = format!("{:?}", f.field);
                let operator = f.operator.storage_variant();
                let contains = f.operator == FilterOperator::Contains;
                match &f.source {
                    FilterSource::Input(name) => {
                        let field_type = input_field(name).map(|f| &f.field_type);
                        let optional = field_type.is_some_and(|t| t.is_option());
                        let is_list = field_type.is_some_and(|t| t.unwrap_option().name == "Vec");
                        PredicateView {
                            field_literal,
                            operator,
                            source: "input",
                            value: name.clone(),
                            conversion: if is_list { "join" } else { "display" },
                            optional,
                            contains,
                        }
                    }
                    FilterSource::Literal(value) => PredicateView {
                        field_literal,
                        operator,
                        source: "literal",
                        value: format!("{:?}", if contains { format!("%{}%", value) } else { value.clone() }),
                        conversion: "display",
                        optional: false,
                        contains: false,
                    },
                    FilterSource::List(values) => PredicateView {
                        field_literal,
                        operator,
                        source: "literal",
                        value: format!("{:?}", values.join(",")),
                        conversion: "display",
                        optional: false,
                        contains: false,
                    },
                    FilterSource::Now | FilterSource::Today(_) => PredicateView {
                        field_literal,
                        operator,
                        source: if f.source == FilterSource::Now { "now" } else { "today" },
                        // The time zone as a Rust string literal (`today("...")`).
                        value: match &f.source {
                            FilterSource::Today(Some(tz)) => format!("{:?}", tz),
                            _ => String::new(),
                        },
                        conversion: "display",
                        optional: false,
                        contains: false,
                    },
                }
            })
            .collect();
        if let Some(owner) = &self.owner {
            predicates.push(PredicateView {
                field_literal: format!("{:?}", owner),
                operator: "Eq",
                source: "user",
                value: String::new(),
                conversion: "display",
                optional: false,
                contains: false,
            });
        }

        let paging = |name: &str| match input_field(name) {
            Some(f) if f.field_type.is_option() => "option",
            Some(_) => "value",
            None => "absent",
        };

        let mut context = Context::new();
        context.insert("predicates", &predicates);
        context.insert("has_optional", &predicates.iter().any(|p| p.optional));
        context.insert("limit", paging("limit"));
        context.insert("offset", paging("offset"));
        render("file/use_case/list_query.rs.tera", &context)
    }

    /// How each `{Entity}Patch` field is filled from the input: the input
    /// field as is (`Option<T>`, the patch's own type), wrapped in `Some`
    /// (an input field of the bare type) or left `None`.
    fn patch_field_views(&self, entity: &RustType, owner: Option<&str>) -> Vec<PatchFieldView> {
        entity
            .fields
            .iter()
            .filter(|f| f.name != "id")
            .map(|f| {
                let source = match self.input.fields.iter().find(|i| i.name == f.name) {
                    // The owner is never patched: it's checked instead.
                    _ if Some(f.name.as_str()) == owner => "none",
                    None => "none",
                    Some(i) if i.field_type.to_call_string(Vec::new()) == f.field_type.to_call_string(Vec::new()) => "some",
                    Some(_) => "input",
                };
                PatchFieldView { name: f.name.clone(), source }
            })
            .collect()
    }

    /// `("field", descending)` pairs for `list_sorted`, as a Rust slice.
    fn order_views(&self) -> Vec<OrderView> {
        self.order_by
            .iter()
            .map(|o| OrderView {
                field: format!("{:?}", o.field),
                descending: o.descending,
            })
            .collect()
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

        let owner = if is_tail { self.owner.as_deref() } else { None };
        let kind = match call.call_kind {
            CallKind::Create => "create",
            CallKind::Read => "read",
            CallKind::Update => "update",
            CallKind::Delete => "delete",
            CallKind::List => "list",
            CallKind::Upsert => "upsert",
            CallKind::Patch => "patch",
            CallKind::SaveOrPatch => "save_or_patch",
        };
        // update_one/delete on an owned entity go through the storage's
        // `*_owned` variant, which checks the owner in the same statement.
        let owned = owner.is_some() && matches!(call.call_kind, CallKind::Update | CallKind::Delete);

        let mut args_context = Context::new();
        args_context.insert("kind", kind);
        args_context.insert("owned", &owned);
        args_context.insert("owner", &owner);
        args_context.insert("owner_literal", &owner.map(|o| format!("{:?}", o)));
        args_context.insert("id_expr", if is_tail { "input.id" } else { "input.id.clone()" });
        args_context.insert("entity", &entity);
        args_context.insert("is_tail", &is_tail);
        // Creations: the infra's `defaults {}` fill what the input lacks.
        let defaults = self.default_fields.get(&call.infra_name).cloned().unwrap_or_default();
        if matches!(call.call_kind, CallKind::Create | CallKind::Upsert | CallKind::SaveOrPatch) && !defaults.is_empty() {
            let mut always = Vec::new();
            let mut optional = Vec::new();
            for field in &defaults {
                match self.input.fields.iter().find(|f| &f.name == field) {
                    Some(f) if f.field_type.is_option() => optional.push(field.clone()),
                    Some(_) => always.push(field.clone()),
                    None => {}
                }
            }
            args_context.insert("defaults_fn", &format!("apply_{}_defaults", call.entity_type.name.to_snake_case()));
            args_context.insert("given_always", &always);
            args_context.insert("given_optional", &optional);
        }
        if call.call_kind == CallKind::SaveOrPatch {
            args_context.insert("patch_name", &format!("{}Patch", call.entity_type.name));
            args_context.insert("patch_fields", &self.patch_field_views(&call.entity_type, owner));
            #[derive(Serialize)]
            struct CreateFieldView {
                name: String,
                /// `id`, `owner`, `flatten`, `now_or`, `unwrap`.
                how: &'static str,
            }
            let mut required = Vec::new();
            let mut create_fields = Vec::new();
            for field in &call.entity_type.fields {
                let how = if field.name == "id" {
                    "id"
                } else if Some(field.name.as_str()) == owner {
                    "owner"
                } else if field.field_type.is_option() {
                    "flatten"
                } else if field.field_type.name == DATE_TIME && !defaults.contains(&field.name) {
                    "now_or"
                } else {
                    if !defaults.contains(&field.name) {
                        required.push(field.name.clone());
                    }
                    "unwrap"
                };
                create_fields.push(CreateFieldView { name: field.name.clone(), how });
            }
            args_context.insert("required", &required);
            args_context.insert("create_fields", &create_fields);
            args_context.insert("error", &self.error_name());
            args_context.insert("output", &self.output_name());
            args_context.insert("field", &field);
        }
        if call.call_kind == CallKind::Patch {
            args_context.insert("patch_name", &format!("{}Patch", call.entity_type.name));
            args_context.insert("patch_fields", &self.patch_field_views(&call.entity_type, owner));
        }
        if call.call_kind == CallKind::List {
            args_context.insert("list_query", &self.list_query_expr());
            args_context.insert("order_by", &if is_tail { self.order_views() } else { Vec::new() });
        }
        let args = render("file/use_case/call_args.rs.tera", &args_context);

        // A sorted list goes through the storage's `list_sorted`.
        let method = if is_tail && call.call_kind == CallKind::List && !self.order_by.is_empty() {
            "list_sorted".to_string()
        } else {
            call.method.clone()
        };
        let mut context = Context::new();
        context.insert("field", &field);
        context.insert("method", &method);
        context.insert("args", &args);
        context.insert("output", &output);
        context.insert("error", &error);
        context.insert("is_tail", &is_tail);
        context.insert("owner", &owner);

        // patch/upsert pass the owner to the storage and get `None` back
        // when no row of theirs matched.
        if matches!(call.call_kind, CallKind::Patch | CallKind::Upsert) {
            context.insert("owner", &None::<String>);
        }
        if call.call_kind == CallKind::SaveOrPatch {
            context.insert("args", &render("file/use_case/call_save_or_patch.rs.tera", &args_context));
            context.insert("owner_literal", &owner.map(|o| format!("{:?}", o)));
            return render("file/use_case/call_save_or_patch_wrapper.rs.tera", &context);
        }
        let template = match call.call_kind {
            _ if owned => "file/use_case/call_owned.rs.tera",
            CallKind::Read | CallKind::Patch | CallKind::Upsert => "file/use_case/call_read.rs.tera",
            _ => "file/use_case/call_simple.rs.tera",
        };
        let code = render(template, &context);
        code
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
        context.insert("generics", &type_params(&self.type_param_names()));
        context.insert("input", &self.input_name());
        context.insert("output", &self.output_name());
        context.insert("error", &self.error_name());
        context.insert("name", &self.name);
        context.insert("where_clause", &where_block(&self.bounds()));
        context.insert("body", &self.gen_execute_body());
        context.insert("trait_name", &self.trait_name());
        context.insert(
            "user_param",
            if self.owner.is_some() {
                "user"
            } else {
                "_user"
            },
        );
        #[derive(Serialize)]
        struct TxFieldView {
            field: String,
            storage: bool,
        }
        let tx_fields: Vec<TxFieldView> = self
            .unique_calls()
            .iter()
            .map(|call| TxFieldView { field: Self::field_name(call), storage: true })
            .collect();
        context.insert("transaction", &self.transaction);
        context.insert("publish", &self.publish);
        context.insert("journal", &self.journal);
        context.insert("tx_first", &tx_fields.first().map(|f| f.field.clone()).unwrap_or_default());
        context.insert("tx_fields", &tx_fields);
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
            let patch = format!("{}Patch", call.entity_type.name);
            if matches!(call.call_kind, CallKind::Patch | CallKind::SaveOrPatch) && !imports.types.contains(&patch) {
                imports.types.push(patch);
            }
        }

        if self.transaction {
            imports.types.push("TrustTransactional".to_string());
        }

        let mut common_items = vec!["UseCase"];
        if self.calls.iter().any(|c| c.call_kind == CallKind::List) {
            common_items.push("StorageListQuery");
        }
        if !self.filters.is_empty()
            || (self.owner.is_some() && self.primary().call_kind == CallKind::List)
        {
            common_items.push("StoragePredicate");
            common_items.push("StoragePredicateOperator");
        }

        let mut code = common_use(&common_items);
        code.push_str(&imports.to_use_statements());
        if self.calls.iter().any(|c| c.call_kind == CallKind::List) {
            code.push_str(&render("file/use_case/list_defaults.rs.tera", &Context::new()));
        }
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
