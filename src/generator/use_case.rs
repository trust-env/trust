use std::{fs::File, path::PathBuf};

use heck::ToSnakeCase;

use crate::{
    ai::AiConfig,
    generator::{
        CodeGen, CodeGenContext, FieldSerde, UseCaseIo,
        rust_code::{
            file::{
                ai_use_case_file::{AiInfraBinding, AiUseCaseFile},
                blob_use_case_file::{BlobOperation, BlobUseCaseFile},
                code_use_case_file::{CodeDependency, CodeUseCaseFile},
                psql_storage_file::STORAGE_ERROR_VARIANTS,
                use_case_file::{CallKind, ResolvedCall, UseCaseFile},
            },
            imports::LocalImports,
            types::{RustField, RustType},
        },
    },
    parser::{AiSpec, AllowedType, BlobCall, CodeSpec, FilterMapping, UseCaseService},
};

pub(super) struct UseCaseShape {
    pub input_type: RustType,
    /// In declaration order; the last entry is primary.
    pub calls: Vec<ResolvedCall>,
    pub copy_id: bool,
    /// Non-empty only when the primary call is `list` and a `filters {}`
    /// block was declared — see `FilterMapping`.
    pub filters: Vec<FilterMapping>,
    /// `owner: <field>` on the primary call's entity, validated to exist
    /// and be `i64`.
    pub owner: Option<String>,
    /// Validated `order_by:` entries of a `list` primary call.
    pub order_by: Vec<crate::parser::OrderBy>,
}

impl CodeGen {
    pub(super) fn resolve_named_type(
        &self,
        context: &CodeGenContext,
        t: &AllowedType,
    ) -> Result<RustType, String> {
        match t {
            AllowedType::DefinedType(name) => context
                .find_type(name)
                .cloned()
                .ok_or_else(|| format!("Undefined type reference: {}", name)),
            _ => Err("UseCase input/output must reference a declared type".to_string()),
        }
    }

    /// When a use case doesn't declare an explicit `input`, guess a shape
    /// from the call kind: `save_one`/`update_one` take the whole entity,
    /// `find_one`/`delete` just need an `id`, and `list` needs paging (plus
    /// one field per `filters {}` entry, if any — each already validated
    /// against `entity_type`'s own fields by the caller).
    /// An `owner` field is never part of a guessed input: it's always
    /// stamped from the caller's JWT, so a client can't choose it.
    fn guess_input_type(
        call_kind: CallKind,
        entity_type: &RustType,
        filters: &[FilterMapping],
        owner: Option<&str>,
    ) -> RustType {
        match call_kind {
            CallKind::Create | CallKind::Update | CallKind::Upsert => {
                let mut guessed = entity_type.clone();
                guessed.fields.retain(|f| Some(f.name.as_str()) != owner);
                guessed
            }
            CallKind::Patch => {
                // `id` plus every other field as `Option`: absent fields
                // are left as they are.
                let mut fields = Vec::new();
                for field in &entity_type.fields {
                    if Some(field.name.as_str()) == owner {
                        continue;
                    }
                    let field_type = if field.name == "id" {
                        field.field_type.clone()
                    } else {
                        RustType::generic("Option", vec![field.field_type.clone()], vec![])
                    };
                    fields.push(RustField { name: field.name.clone(), field_type });
                }
                RustType::generic("GuessedPatchInput", vec![], fields)
            }
            CallKind::Read | CallKind::Delete => {
                let id_field_type = entity_type
                    .fields
                    .iter()
                    .find(|f| f.name == "id")
                    .map(|f| f.field_type.clone())
                    .unwrap_or_else(|| RustType::primitive("i64"));

                RustType::generic(
                    "GuessedLookupInput",
                    vec![],
                    vec![RustField {
                        name: "id".to_string(),
                        field_type: id_field_type,
                    }],
                )
            }
            CallKind::List => {
                let optional_i64 =
                    RustType::generic("Option", vec![RustType::primitive("i64")], vec![]);
                let mut fields = vec![
                    RustField {
                        name: "limit".to_string(),
                        field_type: optional_i64.clone(),
                    },
                    RustField {
                        name: "offset".to_string(),
                        field_type: optional_i64,
                    },
                ];
                for filter in filters {
                    let Some(input_field) = filter.input_field() else {
                        continue;
                    };
                    if fields.iter().any(|f| f.name == input_field) {
                        continue;
                    }
                    // `filter.field` is guaranteed present on `entity_type`
                    // by the validation in `resolve_use_case_shape`.
                    // `in` lists and patterns travel as plain text (a
                    // comma-separated list for `in`), which also works
                    // from a query string.
                    let field_type = if filter.operator.is_set() || filter.operator.is_pattern() {
                        RustType::primitive("String")
                    } else {
                        entity_type
                            .fields
                            .iter()
                            .find(|f| f.name == filter.field)
                            .map(|f| f.field_type.clone())
                            .unwrap_or_else(|| RustType::primitive("String"))
                    };
                    fields.push(RustField {
                        name: input_field.to_string(),
                        field_type,
                    });
                }
                RustType::generic("GuessedListInput", vec![], fields)
            }
        }
    }

    /// Resolves and validates a use case's shape (input/output/calls)
    /// without registering dependencies or writing any file. Used both by a
    /// pre-pass (so adapters declared before their use_case in the `.tr`
    /// file still see accurate `--dev` docs) and by the real generation
    /// step.
    pub(super) fn resolve_use_case_shape(
        &self,
        context: &CodeGenContext,
        use_case: &UseCaseService,
    ) -> Result<Option<UseCaseShape>, String> {
        if use_case.ai.is_some() || use_case.code.is_some() {
            // Handled entirely separately by `generate_ai_use_case` /
            // `generate_code_use_case` / `register_use_case_io`'s own
            // branches — not a CRUD shape.
            return Ok(None);
        }

        if use_case.calls.is_empty() {
            if use_case.input.is_some() || use_case.output.is_some() {
                return Err(format!(
                    "UseCase '{}' declares 'input'/'output' but no 'call'",
                    use_case.name
                ));
            }
            return Ok(None);
        }

        let mut resolved_calls = Vec::new();
        for call in &use_case.calls {
            let entity = context
                .find_infra_entity(&call.target)
                .ok_or_else(|| format!("Unknown infra '{}' referenced in call", call.target))?;
            let entity_type = self.resolve_named_type(context, entity)?;
            let call_kind = CallKind::from_method(&call.method)
                .map_err(|e| format!("UseCase '{}': {}", use_case.name, e))?;

            // The shared `Storage<T, E>` trait (in the external `common`
            // crate) defines find_one/update_one/delete in terms of a
            // fixed `EntityId` — concretely `i64`, not a type parameter
            // tied to whatever the entity's own `id` field declares. An
            // entity with e.g. `id: Uuid` parses and generates fine here,
            // but the generated project then fails to compile the moment
            // this call passes `input.id` to it — a confusing mismatch
            // that only surfaces once someone builds the generated crate
            // against the real `common` checkout. Catch it here instead,
            // with a message that says why. See doc/limitations.md.
            if matches!(call_kind, CallKind::Upsert | CallKind::Patch | CallKind::List)
                && context.find_infra_target_address(&call.target).is_some()
                && call_kind != CallKind::List
            {
                return Err(format!(
                    "UseCase '{}': '{}' needs a psql infra, '{}' is a grpc_client",
                    use_case.name, call.method, call.target
                ));
            }
            if call_kind == CallKind::Upsert && !context.has_psql_unique_key(&call.target) {
                return Err(format!(
                    "UseCase '{}': 'upsert_one' needs a unique key: declare `unique: [...]` on infra '{}'",
                    use_case.name, call.target
                ));
            }
            if matches!(call_kind, CallKind::Read | CallKind::Update | CallKind::Delete | CallKind::Patch)
                && let Some(id_field) = entity_type.fields.iter().find(|f| f.name == "id")
                && id_field.field_type.name != "i64"
            {
                return Err(format!(
                    "UseCase '{}': call '{}.{}' requires entity '{}''s 'id' field to be 'Int' — the underlying Storage trait's EntityId is always i64, so 'id: {}' will fail to compile the generated project",
                    use_case.name, call.target, call.method, entity_type.name, id_field.field_type.name
                ));
            }

            let storage_trait = format!("{}Storage", entity_type.name);
            // `grpc_client` infras back the same `{Entity}Storage` trait
            // as `psql`, but under their own error type (see
            // `GrpcClientStorageFile`) — both use the same two variant
            // names (`STORAGE_ERROR_VARIANTS`), so `errors {}` mapping
            // below needs no further change once the right type name is
            // picked here.
            let storage_error = match context.find_infra_target_address(&call.target) {
                Some(_) => format!("Grpc{}ClientError", entity_type.name),
                None => format!("Psql{}StorageError", entity_type.name),
            };

            for mapping in &call.errors {
                if !STORAGE_ERROR_VARIANTS.contains(&mapping.source.as_str()) {
                    return Err(format!(
                        "UseCase '{}': call '{}.{}''s 'errors' maps unknown source '{}' (expected one of {})",
                        use_case.name, call.target, call.method, mapping.source, STORAGE_ERROR_VARIANTS.join(", ")
                    ));
                }
            }
            for (i, mapping) in call.errors.iter().enumerate() {
                if call.errors[..i].iter().any(|m| m.source == mapping.source) {
                    return Err(format!(
                        "UseCase '{}': call '{}.{}''s 'errors' maps '{}' more than once",
                        use_case.name, call.target, call.method, mapping.source
                    ));
                }
            }

            resolved_calls.push(ResolvedCall {
                infra_name: call.target.clone(),
                entity_type,
                storage_trait,
                storage_error,
                method: call.method.clone(),
                call_kind,
                errors: call.errors.clone(),
            });
        }

        if use_case.transaction {
            if let Some(call) = resolved_calls
                .iter()
                .find(|c| context.find_infra_target_address(&c.infra_name).is_some())
            {
                return Err(format!(
                    "UseCase '{}': 'transaction: true' needs psql infras, '{}' is a grpc_client",
                    use_case.name, call.infra_name
                ));
            }
        }

        let primary_entity = resolved_calls.last().unwrap().entity_type.clone();
        let primary_call_kind = resolved_calls.last().unwrap().call_kind;

        if !use_case.filters.is_empty() {
            if primary_call_kind != CallKind::List {
                return Err(format!(
                    "UseCase '{}': 'filters' is only valid when the primary call is 'list'",
                    use_case.name
                ));
            }
            for filter in &use_case.filters {
                let Some(field) = primary_entity.fields.iter().find(|f| f.name == filter.field) else {
                    return Err(format!(
                        "UseCase '{}': 'filters' references unknown field '{}' on entity '{}'",
                        use_case.name, filter.field, primary_entity.name
                    ));
                };
                if field.field_type.is_json_wrapped() {
                    return Err(format!(
                        "UseCase '{}': 'filters' can't compare '{}' (a JSONB list/struct column)",
                        use_case.name, filter.field
                    ));
                }
            }
        }
        if !use_case.order_by.is_empty() {
            if primary_call_kind != CallKind::List {
                return Err(format!(
                    "UseCase '{}': 'order_by' is only valid when the primary call is 'list'",
                    use_case.name
                ));
            }
            for order in &use_case.order_by {
                if !primary_entity.fields.iter().any(|f| f.name == order.field) {
                    return Err(format!(
                        "UseCase '{}': 'order_by' references unknown field '{}' on entity '{}'",
                        use_case.name, order.field, primary_entity.name
                    ));
                }
            }
            if resolved_calls.last().is_some_and(|c| context.find_infra_target_address(&c.infra_name).is_some()) {
                return Err(format!(
                    "UseCase '{}': 'order_by' needs a psql infra (grpc_client can't sort)",
                    use_case.name
                ));
            }
        }

        if let Some(owner) = &use_case.owner {
            match primary_entity.fields.iter().find(|f| &f.name == owner) {
                None => {
                    return Err(format!(
                        "UseCase '{}': 'owner' references unknown field '{}' on entity '{}'",
                        use_case.name, owner, primary_entity.name
                    ));
                }
                Some(field) if field.field_type.name != "i64" => {
                    return Err(format!(
                        "UseCase '{}': 'owner' field '{}' on entity '{}' must be 'Int' (it holds the JWT's user id)",
                        use_case.name, owner, primary_entity.name
                    ));
                }
                Some(_) => {}
            }
        }

        let (input_type, copy_id) = match &use_case.input {
            Some(input) => {
                let input_type = self.resolve_named_type(context, input)?;
                for resolved in &resolved_calls {
                    for required in resolved.call_kind.required_input_fields() {
                        if !input_type.fields.iter().any(|f| f.name == *required) {
                            return Err(format!(
                                "UseCase '{}': input '{}' must have a '{}' field for call '{}.{}'",
                                use_case.name, input_type.name, required, resolved.infra_name, resolved.method
                            ));
                        }
                    }
                }
                for input_field in use_case.filters.iter().filter_map(|f| f.input_field()) {
                    if !input_type.fields.iter().any(|f| f.name == input_field) {
                        return Err(format!(
                            "UseCase '{}': 'filters' references 'input.{}', but input '{}' has no such field",
                            use_case.name, input_field, input_type.name
                        ));
                    }
                }
                (input_type, false)
            }
            None => {
                if resolved_calls.len() > 1 {
                    return Err(format!(
                        "UseCase '{}': an explicit 'input' is required when declaring more than one 'call'",
                        use_case.name
                    ));
                }
                let guessed = Self::guess_input_type(
                    primary_call_kind,
                    &primary_entity,
                    &use_case.filters,
                    use_case.owner.as_deref(),
                );
                let copy_id = matches!(primary_call_kind, CallKind::Create | CallKind::Update);
                (guessed, copy_id)
            }
        };

        if let Some(output) = &use_case.output {
            let declared_output_type = self.resolve_named_type(context, output)?;
            if primary_entity.name != declared_output_type.name {
                return Err(format!(
                    "UseCase '{}': output type '{}' does not match entity type '{}' returned by the last call ('{}')",
                    use_case.name, declared_output_type.name, primary_entity.name, resolved_calls.last().unwrap().infra_name
                ));
            }
        }

        Ok(Some(UseCaseShape {
            input_type,
            calls: resolved_calls,
            copy_id,
            filters: use_case.filters.clone(),
            owner: use_case.owner.clone(),
            order_by: use_case.order_by.clone(),
        }))
    }

    /// Pre-pass step: records a use case's input/output field summary for
    /// `--dev` docs, independent of when the adapter that uses it is
    /// generated. Safe to call again later (e.g. from `generate_use_case`);
    /// it just overwrites the same entry with identical data.
    fn field_docs(fields: &[RustField]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|f| (f.name.clone(), f.field_type.to_call_string(vec![])))
            .collect()
    }

    /// Name of the built-in raw-request input type (see `RawRequest` in
    /// `templates/domains/raw_request.rs.tera`).
    pub(super) const RAW_REQUEST: &'static str = "RawRequest";

    /// `code {}` input: a declared type, or the built-in `RawRequest` when
    /// no type of that name is declared.
    fn resolve_code_input(
        &self,
        context: &CodeGenContext,
        use_case: &UseCaseService,
    ) -> Result<RustType, String> {
        match &use_case.input {
            Some(AllowedType::DefinedType(name))
                if name == Self::RAW_REQUEST && context.find_type(name).is_none() =>
            {
                Ok(RustType::generic(
                    Self::RAW_REQUEST,
                    vec![],
                    vec![
                        RustField {
                            name: "body".to_string(),
                            field_type: RustType::primitive("String"),
                        },
                        RustField {
                            name: "headers".to_string(),
                            field_type: RustType::primitive("HashMap<String, String>"),
                        },
                    ],
                ))
            }
            Some(t) => self.resolve_named_type(context, t),
            None => Err(format!(
                "UseCase '{}': 'code' requires an explicit 'input'",
                use_case.name
            )),
        }
    }

    fn resolve_code_output(
        &self,
        context: &CodeGenContext,
        use_case: &UseCaseService,
    ) -> Result<RustType, String> {
        match &use_case.output {
            Some(t) => self.resolve_named_type(context, t),
            None => Err(format!(
                "UseCase '{}': 'code' requires an explicit 'output'",
                use_case.name
            )),
        }
    }

    pub(super) fn register_use_case_io(
        &self,
        context: &mut CodeGenContext,
        use_case: &UseCaseService,
    ) -> Result<(), String> {
        if use_case.code.is_some() {
            let input_type = self.resolve_code_input(context, use_case)?;
            let output_type = self.resolve_code_output(context, use_case)?;
            context.register_use_case_io(
                use_case.name.clone(),
                UseCaseIo {
                    input_type_name: input_type.name.clone(),
                    input_fields: Self::field_docs(&input_type.fields),
                    output_type_name: output_type.name.clone(),
                    output_entity: output_type.name.clone(),
                    output_fields: Self::field_docs(&output_type.fields),
                    output_is_list: false,
                    output_wrapper_field: None,
                    requires_user: false,
                    raw_input: input_type.name == Self::RAW_REQUEST
                        && context.find_type(Self::RAW_REQUEST).is_none(),
                    input_serde: FieldSerde::of_fields(&input_type.fields),
                },
            );
            return Ok(());
        }

        if use_case.ai.is_some() {
            let input_type = match &use_case.input {
                Some(t) => self.resolve_named_type(context, t)?,
                None => {
                    return Err(format!(
                        "UseCase '{}': 'ai' requires an explicit 'input'",
                        use_case.name
                    ));
                }
            };
            let output_type = match &use_case.output {
                Some(t) => self.resolve_named_type(context, t)?,
                None => {
                    return Err(format!(
                        "UseCase '{}': 'ai' requires an explicit 'output'",
                        use_case.name
                    ));
                }
            };
            context.register_use_case_io(
                use_case.name.clone(),
                UseCaseIo {
                    input_type_name: input_type.name.clone(),
                    input_fields: Self::field_docs(&input_type.fields),
                    output_type_name: output_type.name.clone(),
                    output_entity: output_type.name.clone(),
                    output_fields: Self::field_docs(&output_type.fields),
                    output_is_list: false,
                    output_wrapper_field: None,
                    requires_user: false,
                    raw_input: false,
                    input_serde: FieldSerde::of_fields(&input_type.fields),
                },
            );
            return Ok(());
        }

        if let Some(blob) = &use_case.blob {
            let operation = BlobOperation::from_operation(&blob.operation)
                .map_err(|e| format!("UseCase '{}': {}", use_case.name, e))?;
            let field_docs = |fields: Vec<(&str, &str)>| {
                fields.into_iter().map(|(n, t)| (n.to_string(), t.to_string())).collect()
            };
            context.register_use_case_io(
                use_case.name.clone(),
                UseCaseIo {
                    input_type_name: format!("{}Input", use_case.name),
                    input_fields: field_docs(BlobOperation::input_fields()),
                    output_type_name: format!("{}Output", use_case.name),
                    output_entity: use_case.name.clone(),
                    output_fields: field_docs(operation.output_fields()),
                    output_is_list: false,
                    output_wrapper_field: None,
                    requires_user: false,
                    raw_input: false,
                    input_serde: Vec::new(),
                },
            );
            return Ok(());
        }

        if let Some(shape) = self.resolve_use_case_shape(context, use_case)? {
            let primary = shape.calls.last().unwrap();
            let is_list = primary.call_kind == CallKind::List;
            let wrapper_field = if is_list {
                format!("{}s", primary.entity_type.name.to_snake_case())
            } else {
                primary.entity_type.name.to_snake_case()
            };
            context.register_use_case_io(
                use_case.name.clone(),
                UseCaseIo {
                    input_type_name: format!("{}Input", use_case.name),
                    input_fields: Self::field_docs(&shape.input_type.fields),
                    output_type_name: format!("{}Output", use_case.name),
                    output_entity: primary.entity_type.name.clone(),
                    output_fields: Self::field_docs(&primary.entity_type.fields),
                    output_is_list: is_list,
                    output_wrapper_field: Some(wrapper_field),
                    requires_user: shape.owner.is_some(),
                    raw_input: false,
                    input_serde: FieldSerde::of_fields(&shape.input_type.fields),
                },
            );
        }
        Ok(())
    }

    /// Describes exactly what an `ai {}` use case's body has available:
    /// the input/output shapes and, if declared, the storage methods it can
    /// call on `self.{field}`. This is the entire "context injection" the
    /// model gets — it never sees the rest of the `.tr` file or codebase.
    fn build_ai_context(
        use_case: &UseCaseService,
        input: &RustType,
        output: &RustType,
        infra: Option<&AiInfraBinding>,
    ) -> String {
        #[derive(serde::Serialize)]
        struct FieldView {
            name: String,
            ty: String,
        }
        #[derive(serde::Serialize)]
        struct TypeView {
            name: String,
            fields: Vec<FieldView>,
        }
        #[derive(serde::Serialize)]
        struct InfraView {
            field: String,
            entity: TypeView,
            storage_trait: String,
            storage_error: String,
        }
        let type_view = |t: &RustType| TypeView {
            name: t.name.clone(),
            fields: t
                .fields
                .iter()
                .map(|f| FieldView { name: f.name.clone(), ty: f.field_type.to_call_string(vec![]) })
                .collect(),
        };

        let mut context = tera::Context::new();
        context.insert("name", &use_case.name);
        context.insert("input", &type_view(input));
        context.insert("output", &type_view(output));
        context.insert(
            "infra",
            &infra.map(|infra| InfraView {
                field: infra.infra_name.to_snake_case(),
                entity: type_view(&infra.entity_type),
                storage_trait: infra.storage_trait.clone(),
                storage_error: infra.storage_error.clone(),
            }),
        );
        crate::generator::rust_code::render::render("ai/context.txt.tera", &context)
    }


    fn generate_ai_use_case(
        &mut self,
        context: &mut CodeGenContext,
        use_case: &UseCaseService,
        ai: &AiSpec,
        folder: PathBuf,
        ai_config: &AiConfig,
    ) -> Result<(), String> {
        let input_type = match &use_case.input {
            Some(t) => self.resolve_named_type(context, t)?,
            None => {
                return Err(format!(
                    "UseCase '{}': 'ai' requires an explicit 'input'",
                    use_case.name
                ));
            }
        };
        let output_type = match &use_case.output {
            Some(t) => self.resolve_named_type(context, t)?,
            None => {
                return Err(format!(
                    "UseCase '{}': 'ai' requires an explicit 'output'",
                    use_case.name
                ));
            }
        };

        let infra = match &ai.infra {
            Some(infra_name) => {
                let entity = context.find_infra_entity(infra_name).ok_or_else(|| {
                    format!("UseCase '{}': 'ai' references unknown infra '{}'", use_case.name, infra_name)
                })?;
                let entity_type = self.resolve_named_type(context, entity)?;
                Some(AiInfraBinding {
                    infra_name: infra_name.clone(),
                    storage_trait: format!("{}Storage", entity_type.name),
                    storage_error: format!("Psql{}StorageError", entity_type.name),
                    entity_type,
                })
            }
            None => None,
        };

        let context_description = Self::build_ai_context(use_case, &input_type, &output_type, infra.as_ref());
        let body = ai_config
            .generate_body(&context_description, &ai.prompt)
            .map_err(|e| format!("UseCase '{}': {}", use_case.name, e))?;

        context.require_git_dependency("common", "ssh://git@github.com/BH-World/common.git");
        context.require_version_dependency_with_features("serde", "1", vec!["derive"]);

        let mut file = AiUseCaseFile::new(use_case.name.clone(), input_type, output_type, infra, body, folder)?;
        file.write()?;
        context.register_use_case_module(use_case.name.to_snake_case());
        Ok(())
    }

    /// `blob:` binds a use case straight to one `s3 {}` infra operation —
    /// no CRUD `call:` chain, no `errors {}` customization (same precedent
    /// `ai {}` sets: a fixed `{NotFound, StorageError}` enum, see
    /// `BlobUseCaseFile`'s doc comment). Input/output are always the fixed
    /// synthetic shape `BlobOperation` defines, so — unlike CRUD, where an
    /// explicit `input`/`output` narrows an otherwise-guessed shape — a
    /// `blob {}` use case declaring its own is a hard error, since there's
    /// nothing for it to override.
    fn generate_blob_use_case(
        &mut self,
        context: &mut CodeGenContext,
        use_case: &UseCaseService,
        blob: &BlobCall,
        folder: PathBuf,
    ) -> Result<(), String> {
        if use_case.input.is_some() || use_case.output.is_some() {
            return Err(format!(
                "UseCase '{}': 'blob' use cases have a fixed input/output shape and cannot declare their own",
                use_case.name
            ));
        }

        let operation = BlobOperation::from_operation(&blob.operation)
            .map_err(|e| format!("UseCase '{}': {}", use_case.name, e))?;

        if !context.is_blob_infra(&blob.target) {
            return Err(format!(
                "UseCase '{}': 'blob' references '{}', which isn't an s3 {{}} infra",
                use_case.name, blob.target
            ));
        }
        let entity = context.find_blob_infra_entity(&blob.target).ok_or_else(|| {
            format!("UseCase '{}': 'blob' references unknown infra '{}'", use_case.name, blob.target)
        })?;
        let entity_type = self.resolve_named_type(context, entity)?;
        let storage_type = format!("S3{}Storage", entity_type.name);
        let infra_field = blob.target.to_snake_case();

        context.require_git_dependency("common", "ssh://git@github.com/BH-World/common.git");
        context.require_version_dependency_with_features("serde", "1", vec!["derive"]);

        let mut file = BlobUseCaseFile::new(use_case.name.clone(), operation, infra_field, storage_type, folder)?;
        file.write()?;
        context.register_use_case_module(use_case.name.to_snake_case());
        Ok(())
    }

    /// `code {}`: the body is read from `code.file` (resolved against the
    /// working directory, like every other path in a `.tr` file) and
    /// spliced into `execute`, with one struct field per `infra:`.
    fn generate_code_use_case(
        &mut self,
        context: &mut CodeGenContext,
        use_case: &UseCaseService,
        code: &CodeSpec,
        folder: PathBuf,
    ) -> Result<(), String> {
        let input_type = self.resolve_code_input(context, use_case)?;
        let output_type = self.resolve_code_output(context, use_case)?;

        let mut dependencies = Vec::new();
        for infra_name in &code.infras {
            if code.infras.iter().filter(|n| *n == infra_name).count() > 1 {
                return Err(format!(
                    "UseCase '{}': 'code' lists infra '{}' more than once",
                    use_case.name, infra_name
                ));
            }
            let dependency = if context.find_http_client_infra(infra_name).is_some() {
                CodeDependency::Concrete {
                    infra_name: infra_name.clone(),
                    type_name: Self::http_client_struct_name(infra_name),
                }
            } else if context.is_blob_infra(infra_name) {
                let entity = context.find_blob_infra_entity(infra_name).ok_or_else(|| {
                    format!(
                        "UseCase '{}': unknown infra '{}'",
                        use_case.name, infra_name
                    )
                })?;
                let entity_type = self.resolve_named_type(context, entity)?;
                CodeDependency::Concrete {
                    infra_name: infra_name.clone(),
                    type_name: format!("S3{}Storage", entity_type.name),
                }
            } else {
                let entity = context.find_infra_entity(infra_name).ok_or_else(|| {
                    format!(
                        "UseCase '{}': 'code' references unknown infra '{}'",
                        use_case.name, infra_name
                    )
                })?;
                let entity_type = self.resolve_named_type(context, entity)?;
                let storage_error = match context.find_infra_target_address(infra_name) {
                    Some(_) => format!("Grpc{}ClientError", entity_type.name),
                    None => format!("Psql{}StorageError", entity_type.name),
                };
                let transactional = context.find_infra_target_address(infra_name).is_none();
                CodeDependency::Storage {
                    infra_name: infra_name.clone(),
                    storage_trait: format!("{}Storage", entity_type.name),
                    storage_error,
                    transactional,
                }
            };
            dependencies.push(dependency);
        }

        for reserved in ["NotFound", "StorageError"] {
            if code.errors.iter().any(|e| e == reserved) {
                return Err(format!(
                    "UseCase '{}': '{}' is always part of the error enum — don't declare it in 'errors'",
                    use_case.name, reserved
                ));
            }
        }

        if use_case.transaction {
            if !dependencies.iter().any(|d| matches!(d, CodeDependency::Storage { transactional: true, .. })) {
                return Err(format!(
                    "UseCase '{}': 'transaction: true' needs at least one psql infra",
                    use_case.name
                ));
            }
            if let Some(CodeDependency::Storage { infra_name, .. }) = dependencies
                .iter()
                .find(|d| matches!(d, CodeDependency::Storage { transactional: false, .. }))
            {
                return Err(format!(
                    "UseCase '{}': 'transaction: true' can't include grpc_client infra '{}'",
                    use_case.name, infra_name
                ));
            }
        }

        for include in &code.includes {
            self.write_shared_module(context, &use_case.name, include, &folder)?;
        }

        let body = std::fs::read_to_string(&code.file).map_err(|e| {
            format!(
                "UseCase '{}': failed to read code file '{}': {}",
                use_case.name, code.file, e
            )
        })?;

        context.require_git_dependency("common", "ssh://git@github.com/BH-World/common.git");
        context.require_version_dependency_with_features("serde", "1", vec!["derive"]);
        for (name, version) in &code.deps {
            context.require_version_dependency(name, version);
        }

        if input_type.name == Self::RAW_REQUEST && context.find_type(Self::RAW_REQUEST).is_none() {
            let domains = folder
                .parent()
                .ok_or("applications folder has no parent")?
                .join("domains");
            std::fs::write(
                domains.join("raw_request.rs"),
                crate::generator::rust_code::render::render(
                    "domains/raw_request.rs.tera",
                    &tera::Context::new(),
                ),
            )
            .map_err(|e| format!("Failed to write RawRequest: {}", e))?;
            context.register_domain_module("raw_request".to_string());
        }

        CodeUseCaseFile {
            name: use_case.name.clone(),
            input: input_type.name,
            output: output_type.name,
            source_file: code.file.clone(),
            body,
            dependencies,
            error_variants: code.errors.clone(),
            transaction: use_case.transaction,
            publish: context.has_events() && use_case.notifies(),
        }
        .write(&folder)?;
        context.register_use_case_module(use_case.name.to_snake_case());
        Ok(())
    }

    /// Copies a `code { include: "..." }` file into `src/shared/<stem>.rs`
    /// (once per crate); its `pub` items end up at the crate root, where
    /// every `code {}` body sees them.
    fn write_shared_module(
        &self,
        context: &mut CodeGenContext,
        use_case_name: &str,
        include: &str,
        use_case_folder: &PathBuf,
    ) -> Result<(), String> {
        let stem = std::path::Path::new(include)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_snake_case())
            .ok_or_else(|| format!("UseCase '{}': invalid include path '{}'", use_case_name, include))?;
        match context.find_shared_module(&stem) {
            Some(existing) if existing == include => return Ok(()),
            Some(existing) => {
                return Err(format!(
                    "UseCase '{}': include '{}' and '{}' would both become module '{}' — rename one",
                    use_case_name, include, existing, stem
                ));
            }
            None => {}
        }
        let source = std::fs::read_to_string(include).map_err(|e| {
            format!("UseCase '{}': failed to read include '{}': {}", use_case_name, include, e)
        })?;
        let shared_dir = use_case_folder
            .parent()
            .ok_or("applications folder has no parent")?
            .join("shared");
        std::fs::create_dir_all(&shared_dir)
            .map_err(|e| format!("Failed to create the shared folder: {}", e))?;
        let mut tera_context = tera::Context::new();
        tera_context.insert("file", include);
        tera_context.insert("source", &source);
        std::fs::write(
            shared_dir.join(format!("{}.rs", stem)),
            crate::generator::rust_code::render::render("file/code_use_case/shared.rs.tera", &tera_context),
        )
        .map_err(|e| format!("Failed to write shared module '{}': {}", stem, e))?;
        context.register_shared_module(stem, include.to_string());
        Ok(())
    }

    pub(super) fn generate_use_case(
        &mut self,
        context: &mut CodeGenContext,
        use_case: UseCaseService,
        folder: PathBuf,
        ai_config: &AiConfig,
    ) -> Result<(), String> {
        if let Some(ai) = use_case.ai.clone() {
            return self.generate_ai_use_case(context, &use_case, &ai, folder, ai_config);
        }
        if let Some(blob) = use_case.blob.clone() {
            return self.generate_blob_use_case(context, &use_case, &blob, folder);
        }
        if let Some(code) = use_case.code.clone() {
            return self.generate_code_use_case(context, &use_case, &code, folder);
        }

        match self.resolve_use_case_shape(context, &use_case)? {
            Some(shape) => {
                context.require_git_dependency(
                    "common",
                    "ssh://git@github.com/BH-World/common.git",
                );
                context.require_version_dependency_with_features("serde", "1", vec!["derive"]);
                if LocalImports::from_fields(&shape.input_type.fields, &shape.input_type.name)
                    .needs_uuid
                {
                    context.require_version_dependency_with_features("uuid", "1", vec!["serde"]);
                }

                if shape
                    .input_type
                    .fields
                    .iter()
                    .any(|f| f.field_type.is_option() && f.field_type.unwrap_option().is_option())
                {
                    context.require_double_option();
                }
                for filter in &shape.filters {
                    match &filter.source {
                        crate::parser::FilterSource::Now | crate::parser::FilterSource::Today(None) => {
                            context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
                        }
                        crate::parser::FilterSource::Today(Some(tz)) => {
                            tz.parse::<chrono_tz::Tz>().map_err(|_| {
                                format!("UseCase '{}': unknown time zone '{}' in today(...)", use_case.name, tz)
                            })?;
                            context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
                            context.require_version_dependency("chrono-tz", "0.10");
                        }
                        _ => {}
                    }
                }
                let mut file = UseCaseFile::new(
                    use_case.name.clone(),
                    shape.input_type,
                    shape.calls,
                    shape.copy_id,
                    shape.filters,
                    shape.owner,
                    shape.order_by,
                    use_case.transaction,
                    folder,
                )?
                .with_publish(context.has_events() && use_case.notifies())
                .with_json_camel(context.json_camel());

                file.write()?;
                context.register_use_case_module(use_case.name.to_snake_case());
                Ok(())
            }
            None => {
                let module_name = use_case.name.to_snake_case();
                let file_path = folder.join(format!("{}.rs", module_name));

                File::create(&file_path).map_err(|e| {
                    format!("error when creating the file {:?}: {:?}", file_path, e)
                })?;
                context.register_use_case_module(module_name);
                Ok(())
            }
        }
    }
}

