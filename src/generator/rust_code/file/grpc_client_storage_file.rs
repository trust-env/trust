use std::{fs::File, io::Write, path::PathBuf};

use heck::ToSnakeCase;

use crate::generator::rust_code::{
    file::grpc_adapter_file::GrpcRpcBinding, proto, render::render, types::RustType,
};
use serde::Serialize;
use tera::Context;

/// The fixed set of `Storage<T, E>` verbs every `grpc_client` infra must
/// implement (mirrors `common::Storage`'s method list). Only `find_one` has
/// real codegen today (see `gen_grpc_client_storage`'s explicit check) —
/// every other verb always renders as an "unsupported" stub, so the struct
/// still satisfies the full trait.
const STORAGE_VERBS: &[&str] =
    &["save_one", "save_many", "find_one", "find_many", "list", "update_one", "delete", "delete_many"];

/// Writes a `Grpc{Entity}Client` implementing the same `Storage<T, E>`
/// trait `psql` storage implements (see `psql_storage_file.rs`), so a
/// `use_case { call: RemoteInfra.find_one }` works against it exactly like
/// it would against a local `psql` infra — the only difference is that
/// `find_one` makes a real gRPC call to another app's `grpc {}` adapter
/// instead of a SQL query.
pub struct GrpcClientStorageFile {
    entity: RustType,
    error_name: String,
    struct_name: String,
    /// The target adapter's own name/package, exactly as the server side
    /// computed them — needed so this crate's own duplicate `.proto`
    /// declares the same `package`/`service` the target's compiled server
    /// stub answers to, and so `tonic::include_proto!` resolves correctly.
    service_name: String,
    package: String,
    /// One entry per `methods { <verb>: <rpc> }` mapping, in declaration
    /// order: the `Storage` verb name paired with that rpc's resolved wire
    /// shape (reusing the exact same resolution the server side uses).
    bindings: Vec<(String, GrpcRpcBinding)>,
    proto_path: PathBuf,
    rust_file: File,
}

impl GrpcClientStorageFile {
    pub fn new(
        entity: RustType,
        service_name: String,
        _target_address: String,
        bindings: Vec<(String, GrpcRpcBinding)>,
        proto_dir: PathBuf,
        rust_folder: PathBuf,
    ) -> Result<Self, String> {
        let package = service_name.to_snake_case();
        // Named after this infra's own entity (not `package`) to avoid
        // colliding with a same-named file another `grpc_client` infra in
        // this crate might write for a different target.
        let proto_path = proto_dir.join(format!("{}_client.proto", entity.name.to_snake_case()));
        let rust_path = rust_folder.join(format!("{}.rs", entity.to_file_name()));

        Ok(GrpcClientStorageFile {
            error_name: format!("Grpc{}ClientError", entity.name),
            struct_name: format!("Grpc{}Client", entity.name),
            rust_file: File::create(&rust_path)
                .map_err(|e| format!("Failed to create grpc client storage file: {}", e))?,
            entity,
            service_name,
            package,
            bindings,
            proto_path,
        })
    }

    pub fn proto_path(&self) -> &PathBuf {
        &self.proto_path
    }

    pub fn module_name(&self) -> String {
        self.entity.to_file_name()
    }

    fn binding_for(&self, verb: &str) -> Option<&GrpcRpcBinding> {
        self.bindings.iter().find(|(v, _)| v == verb).map(|(_, b)| b)
    }

    /// Renders one `message` block, deduped by name, appending to `seen`/
    /// `messages` — mirrors `GrpcAdapterFile::push_flat_message`.
    fn push_flat_message(
        &self,
        seen: &mut Vec<String>,
        messages: &mut Vec<String>,
        name: &str,
        fields: &[(String, String)],
    ) -> Result<(), String> {
        if seen.contains(&name.to_string()) {
            return Ok(());
        }
        seen.push(name.to_string());
        let rendered = proto::render_message(name, fields)
            .map_err(|e| format!("grpc_client '{}': message '{}': {}", self.service_name, name, e))?;
        messages.push(rendered);
        Ok(())
    }

    fn write_proto(&self) -> Result<(), String> {
        #[derive(Serialize)]
        struct RpcLineView {
            name: String,
            input_type_name: String,
            output_type_name: String,
        }

        let mut seen = Vec::new();
        let mut messages = Vec::new();
        for (_, binding) in &self.bindings {
            self.push_flat_message(&mut seen, &mut messages, &binding.input_type_name, &binding.input_fields)?;

            match &binding.output_wrapper_field {
                Some(wrapper_field) => {
                    self.push_flat_message(&mut seen, &mut messages, &binding.output_entity, &binding.output_fields)?;
                    if !seen.contains(&binding.output_type_name) {
                        seen.push(binding.output_type_name.clone());
                        messages.push(proto::render_wrapper_message(
                            &binding.output_type_name,
                            wrapper_field,
                            &binding.output_entity,
                        ));
                    }
                }
                None => {
                    self.push_flat_message(&mut seen, &mut messages, &binding.output_type_name, &binding.output_fields)?
                }
            }
        }

        let rpcs: Vec<RpcLineView> = self
            .bindings
            .iter()
            .map(|(_, b)| RpcLineView {
                name: b.name.clone(),
                input_type_name: b.input_type_name.clone(),
                output_type_name: b.output_type_name.clone(),
            })
            .collect();

        let mut context = Context::new();
        context.insert("package", &self.package);
        context.insert("messages", &messages);
        context.insert("name", &self.service_name);
        context.insert("rpcs", &rpcs);
        let code = render("file/grpc_adapter/proto.proto.tera", &context);

        std::fs::write(&self.proto_path, code).map_err(|e| format!("Failed to write {:?}: {}", self.proto_path, e))
    }

    fn gen_find_one_method(&self, binding: &GrpcRpcBinding) -> String {
        let fields: Vec<String> = binding.output_fields.iter().map(|(name, _)| name.clone()).collect();

        let mut context = Context::new();
        context.insert("entity", &self.entity.name);
        context.insert("error_name", &self.error_name);
        context.insert("input_type", &binding.input_type_name);
        context.insert("rpc_method", &binding.name.to_snake_case());
        context.insert("has_wrapper", &binding.output_wrapper_field.is_some());
        context.insert("wrapper_field", binding.output_wrapper_field.as_deref().unwrap_or(""));
        context.insert("fields", &fields);
        render("file/grpc_client_storage/find_one_method.rs.tera", &context)
    }

    fn gen_unsupported_method(&self, verb: &str) -> String {
        assert!(STORAGE_VERBS.contains(&verb), "not one of STORAGE_VERBS: {}", verb);
        let mut context = Context::new();
        context.insert("entity", &self.entity.name);
        context.insert("error_name", &self.error_name);
        context.insert("verb", verb);
        render("file/grpc_client_storage/unsupported_method.rs.tera", &context)
    }

    fn gen_methods_code(&self) -> String {
        STORAGE_VERBS
            .iter()
            .map(|verb| match self.binding_for(verb) {
                Some(binding) if *verb == "find_one" => self.gen_find_one_method(binding),
                _ => self.gen_unsupported_method(verb),
            })
            .collect()
    }

    pub fn write(&mut self) -> Result<(), String> {
        self.write_proto()?;

        let client_module = format!("{}_client", self.package);
        let mut context = Context::new();
        context.insert("entity", &self.entity.name);
        context.insert("error_name", &self.error_name);
        context.insert("struct_name", &self.struct_name);
        context.insert("package", &self.package);
        context.insert("client_module", &client_module);
        context.insert("service_name", &self.service_name);
        context.insert("methods_code", &self.gen_methods_code());
        #[derive(serde::Serialize)]
        struct PatchFieldView {
            name: String,
            rust_type: String,
        }
        let patch_fields: Vec<PatchFieldView> = self
            .entity
            .fields
            .iter()
            .filter(|f| f.name != "id")
            .map(|f| PatchFieldView {
                name: f.name.clone(),
                rust_type: f.field_type.to_call_string(Vec::new()),
            })
            .collect();
        context.insert("patch_fields", &patch_fields);
        let code = render("file/grpc_client_storage/client.rs.tera", &context);

        self.rust_file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write grpc client storage file: {}", e))
    }
}
