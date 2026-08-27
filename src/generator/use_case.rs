use std::{fs::File, path::PathBuf};

use heck::ToSnakeCase;

use crate::{
    ai::AiConfig,
    generator::{
        CodeGen, CodeGenContext, UseCaseIo,
        rust_code::{
            file::{
                ai_use_case_file::{AiInfraBinding, AiUseCaseFile},
                blob_use_case_file::{BlobOperation, BlobUseCaseFile},
                psql_storage_file::STORAGE_ERROR_VARIANTS,
                use_case_file::{CallKind, ResolvedCall, UseCaseFile},
            },
            imports::LocalImports,
            types::{RustField, RustType},
        },
    },
    parser::{AiSpec, AllowedType, BlobCall, FilterMapping, UseCaseService},
};

pub(super) struct UseCaseShape {
    pub input_type: RustType,
    /// In declaration order; the last entry is primary.
    pub calls: Vec<ResolvedCall>,
    pub copy_id: bool,
    /// Non-empty only when the primary call is `list` and a `filters {}`
    /// block was declared — see `FilterMapping`.
    pub filters: Vec<FilterMapping>,
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
    fn guess_input_type(
        call_kind: CallKind,
        entity_type: &RustType,
        filters: &[FilterMapping],
    ) -> RustType {
        match call_kind {
            CallKind::Create | CallKind::Update => entity_type.clone(),
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
                let mut fields = vec![
                    RustField {
                        name: "limit".to_string(),
                        field_type: RustType::primitive("i64"),
                    },
                    RustField {
                        name: "offset".to_string(),
                        field_type: RustType::primitive("i64"),
                    },
                ];
                for filter in filters {
                    // `filter.field` is guaranteed present on `entity_type`
                    // by the validation in `resolve_use_case_shape`.
                    let field_type = entity_type
                        .fields
                        .iter()
                        .find(|f| f.name == filter.field)
                        .map(|f| f.field_type.clone())
                        .unwrap_or_else(|| RustType::primitive("String"));
                    fields.push(RustField {
                        name: filter.input_field.clone(),
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
        if use_case.ai.is_some() {
            // Handled entirely separately by `generate_ai_use_case` /
            // `register_use_case_io`'s own branch — not a CRUD shape.
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
            if matches!(call_kind, CallKind::Read | CallKind::Update | CallKind::Delete)
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
                if !primary_entity.fields.iter().any(|f| f.name == filter.field) {
                    return Err(format!(
                        "UseCase '{}': 'filters' references unknown field '{}' on entity '{}'",
                        use_case.name, filter.field, primary_entity.name
                    ));
                }
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
                for filter in &use_case.filters {
                    if !input_type
                        .fields
                        .iter()
                        .any(|f| f.name == filter.input_field)
                    {
                        return Err(format!(
                            "UseCase '{}': 'filters' references 'input.{}', but input '{}' has no such field",
                            use_case.name, filter.input_field, input_type.name
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
                let guessed =
                    Self::guess_input_type(primary_call_kind, &primary_entity, &use_case.filters);
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

    pub(super) fn register_use_case_io(
        &self,
        context: &mut CodeGenContext,
        use_case: &UseCaseService,
    ) -> Result<(), String> {
        if use_case.ai.is_some() {
            let input_type = match &use_case.input {
                Some(t) => self.resolve_named_type(context, t)?,
                None => return Err(format!("UseCase '{}': 'ai' requires an explicit 'input'", use_case.name)),
            };
            let output_type = match &use_case.output {
                Some(t) => self.resolve_named_type(context, t)?,
                None => return Err(format!("UseCase '{}': 'ai' requires an explicit 'output'", use_case.name)),
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
        let field_list = |t: &RustType| -> String {
            t.fields
                .iter()
                .map(|f| format!("  {}: {}", f.name, f.field_type.to_call_string(vec![])))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let mut ctx = String::new();
        ctx.push_str(&format!(
            "You are implementing: async fn execute(&self, input: {}) -> Result<{}, {}Error>\n\n",
            input.name, output.name, use_case.name
        ));
        ctx.push_str(&format!("Input struct `{}` fields:\n{}\n\n", input.name, field_list(input)));
        ctx.push_str(&format!(
            "Output struct `{}` fields (construct it with `{} {{ field: value, ... }}`):\n{}\n\n",
            output.name, output.name, field_list(output)
        ));
        ctx.push_str(&format!(
            "Error enum `{name}Error` — the only variants you may return: {name}Error::NotFound, {name}Error::StorageError(String)\n\n",
            name = use_case.name
        ));

        match infra {
            Some(infra) => {
                ctx.push_str(&format!(
                    "You have `self.{field}`, a storage handle for entity `{entity}` (fields:\n{entity_fields}\n) implementing the `{trait}` trait, with these async methods (each returns `Result<_, {storage_error}>` — map errors with `.map_err({name}Error::from)`):\n\
  save_one(entity: {entity}) -> {entity}\n\
  save_many(entities: Vec<{entity}>) -> Vec<{entity}>\n\
  find_one(id: i64) -> Option<{entity}>\n\
  find_many(ids: Vec<i64>) -> Vec<{entity}>\n\
  list(query: common::StorageListQuery) -> Vec<{entity}>\n\
  update_one(id: i64, entity: {entity}) -> {entity}\n\
  delete(id: i64) -> {entity}\n\
  delete_many(ids: Vec<i64>) -> Vec<{entity}>\n\n",
                    field = infra.infra_name.to_snake_case(),
                    entity = infra.entity_type.name,
                    entity_fields = field_list(&infra.entity_type),
                    trait = infra.storage_trait,
                    storage_error = infra.storage_error,
                    name = use_case.name,
                ));
            }
            None => {
                ctx.push_str("No storage is available here — implement this purely from `input`.\n\n");
            }
        }
        ctx
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
            None => return Err(format!("UseCase '{}': 'ai' requires an explicit 'input'", use_case.name)),
        };
        let output_type = match &use_case.output {
            Some(t) => self.resolve_named_type(context, t)?,
            None => return Err(format!("UseCase '{}': 'ai' requires an explicit 'output'", use_case.name)),
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

                let mut file = UseCaseFile::new(
                    use_case.name.clone(),
                    shape.input_type,
                    shape.calls,
                    shape.copy_id,
                    shape.filters,
                    folder,
                )?;

                file.write()?;
                context.register_use_case_module(use_case.name.to_snake_case());
                Ok(())
            }
            None => {
                let module_name = use_case.name.to_snake_case();
                let file_path = folder.join(format!("{}.rs", module_name));

                File::create(&file_path)
                    .map_err(|e| format!("error when creating the file {:?}: {:?}", file_path, e))?;
                context.register_use_case_module(module_name);
                Ok(())
            }
        }
    }
}
