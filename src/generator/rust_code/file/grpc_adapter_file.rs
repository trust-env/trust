use std::{fs::File, io::Write, path::PathBuf};

use heck::ToSnakeCase;

use crate::generator::rust_code::{
    fragments::{params as fragment_params, shared_bound, where_clause},
    imports::crate_use,
    proto,
    render::render,
};
use serde::Serialize;
use tera::Context;

/// One resolved `rpc Name { input: X output: Y use_case: Z }` — the gRPC
/// equivalent of an `HttpRouteBinding`.
pub struct GrpcRpcBinding {
    pub name: String,
    pub use_case_name: String,
    pub use_case_field: String,
    pub type_param: String,
    pub input_type_name: String,
    pub input_fields: Vec<(String, String)>,
    pub output_type_name: String,
    pub output_entity: String,
    pub output_fields: Vec<(String, String)>,
    /// See `UseCaseIo::output_wrapper_field` — `Some(field)` for CRUD use
    /// cases (the real output type has exactly one field, `field`, holding
    /// the entity), `None` for `ai {}` use cases (flat, `output_fields`
    /// exactly).
    pub output_wrapper_field: Option<String>,
    /// `{UseCaseName}Error` — every use_case error enum always has a bare
    /// `NotFound` variant (`file/use_case/error.rs.tera`), so the rpc
    /// method can map it to `Status::not_found` specifically instead of
    /// collapsing every error into `Status::internal`.
    pub error_type: String,
}

pub struct GrpcAdapterFile {
    name: String,
    package: String,
    rpcs: Vec<GrpcRpcBinding>,
    proto_path: PathBuf,
    rust_file: File,
}

impl GrpcAdapterFile {
    pub fn new(name: String, rpcs: Vec<GrpcRpcBinding>, proto_dir: PathBuf, rust_folder: PathBuf) -> Result<Self, String> {
        let package = name.to_snake_case();
        let proto_path = proto_dir.join(format!("{}.proto", package));
        let rust_path = rust_folder.join(format!("{}.rs", package));

        Ok(GrpcAdapterFile {
            rust_file: File::create(&rust_path)
                .map_err(|e| format!("Failed to create grpc adapter file: {}", e))?,
            name,
            package,
            rpcs,
            proto_path,
        })
    }

    pub fn proto_path(&self) -> &PathBuf {
        &self.proto_path
    }

    pub fn package(&self) -> &str {
        &self.package
    }

    fn unique_bindings(&self) -> Vec<&GrpcRpcBinding> {
        let mut seen = Vec::new();
        let mut result = Vec::new();
        for rpc in &self.rpcs {
            if !seen.contains(&rpc.type_param) {
                seen.push(rpc.type_param.clone());
                result.push(rpc);
            }
        }
        result
    }

    fn generics(&self) -> String {
        self.unique_bindings().iter().map(|b| b.type_param.clone()).collect::<Vec<_>>().join(", ")
    }

    fn where_clause(&self) -> String {
        let bounds: Vec<(String, Vec<String>)> = self
            .unique_bindings()
            .iter()
            .map(|b| (b.type_param.clone(), shared_bound(&format!("{}Trait", b.use_case_name))))
            .collect();
        where_clause(&bounds)
    }

    fn push_flat_message(
        seen: &mut Vec<String>,
        result: &mut Vec<(String, Result<String, (String, String)>)>,
        name: &str,
        fields: &[(String, String)],
    ) {
        if seen.contains(&name.to_string()) {
            return;
        }
        seen.push(name.to_string());
        let rendered = proto::render_message(name, fields).map_err(|e| (name.to_string(), e));
        result.push((name.to_string(), rendered));
    }

    /// One proto `message` this adapter needs to emit, per unique message
    /// name. The output side of a CRUD use case needs two: the entity
    /// itself (flat scalar fields) and a one-field wrapper referencing it
    /// by message type rather than a scalar — see
    /// `GrpcRpcBinding::output_wrapper_field`.
    fn message_specs(&self) -> Vec<(String, Result<String, (String, String)>)> {
        let mut seen = Vec::new();
        let mut result: Vec<(String, Result<String, (String, String)>)> = Vec::new();

        for rpc in &self.rpcs {
            Self::push_flat_message(&mut seen, &mut result, &rpc.input_type_name, &rpc.input_fields);

            match &rpc.output_wrapper_field {
                Some(field_name) => {
                    Self::push_flat_message(&mut seen, &mut result, &rpc.output_entity, &rpc.output_fields);
                    if !seen.contains(&rpc.output_type_name) {
                        seen.push(rpc.output_type_name.clone());
                        let rendered = proto::render_wrapper_message(&rpc.output_type_name, field_name, &rpc.output_entity);
                        result.push((rpc.output_type_name.clone(), Ok(rendered)));
                    }
                }
                None => Self::push_flat_message(&mut seen, &mut result, &rpc.output_type_name, &rpc.output_fields),
            }
        }
        result
    }

    fn write_proto(&self) -> Result<(), String> {
        #[derive(Serialize)]
        struct RpcLineView {
            name: String,
            input_type_name: String,
            output_type_name: String,
        }

        let mut messages = Vec::new();
        for (_, rendered) in self.message_specs() {
            match rendered {
                Ok(block) => messages.push(block),
                Err((name, error)) => {
                    return Err(format!("gRPC adapter '{}': message '{}': {}", self.name, name, error));
                }
            }
        }
        let rpcs: Vec<RpcLineView> = self
            .rpcs
            .iter()
            .map(|rpc| RpcLineView {
                name: rpc.name.clone(),
                input_type_name: rpc.input_type_name.clone(),
                output_type_name: rpc.output_type_name.clone(),
            })
            .collect();

        let mut context = Context::new();
        context.insert("package", &self.package);
        context.insert("messages", &messages);
        context.insert("name", &self.name);
        context.insert("rpcs", &rpcs);
        let code = render("file/grpc_adapter/proto.proto.tera", &context);

        std::fs::write(&self.proto_path, code)
            .map_err(|e| format!("Failed to write {:?}: {}", self.proto_path, e))
    }

    fn gen_imports(&self) -> String {
        let messages: Vec<String> = self.message_specs().into_iter().map(|(name, _)| name).collect();
        let mut header_context = Context::new();
        header_context.insert("package", &self.package);
        header_context.insert("name", &self.name);
        header_context.insert("messages", &messages);
        let mut code = render("file/grpc_adapter/header.rs.tera", &header_context);

        let mut crate_items: Vec<String> = self
            .unique_bindings()
            .iter()
            .map(|b| format!("{}Trait", b.use_case_name))
            .collect();
        for rpc in &self.rpcs {
            // Only the domain input type is ever constructed directly here
            // (`{Input} { field: request.field, ... }`); the output comes
            // back already-typed from `execute()`, and only its *proto*
            // counterpart gets constructed.
            crate_items.push(rpc.input_type_name.clone());
            // Needed to pattern-match `NotFound` out of `execute()`'s error
            // in `rpc_method.rs.tera` (see `Self::rpc_method_name`'s
            // caller).
            crate_items.push(rpc.error_type.clone());
        }
        crate_items.sort();
        crate_items.dedup();
        code.push_str(&crate_use(&crate_items));
        code.push('\n');
        code
    }

    fn gen_struct(&self) -> String {
        #[derive(Serialize)]
        struct UseCaseFieldView {
            field: String,
            type_param: String,
        }

        let use_case_fields: Vec<UseCaseFieldView> = self
            .unique_bindings()
            .iter()
            .map(|b| UseCaseFieldView {
                field: b.use_case_field.clone(),
                type_param: b.type_param.clone(),
            })
            .collect();

        // Reuses http_adapter's struct template — identical shape, just
        // without any auth fields (gRPC adapters don't have `auth {}`).
        let mut struct_context = Context::new();
        struct_context.insert("name", &self.name);
        struct_context.insert("generics", &self.generics());
        struct_context.insert("where_clause", &self.where_clause());
        struct_context.insert("use_case_fields", &use_case_fields);
        struct_context.insert("auth_fields", &Vec::<serde_json::Value>::new());
        let mut code = render("file/http_adapter/struct.rs.tera", &struct_context);

        let params: Vec<(String, String)> = self
            .unique_bindings()
            .iter()
            .map(|b| (b.use_case_field.clone(), b.type_param.clone()))
            .collect();
        let params = fragment_params(&params);
        let fields: Vec<String> = self.unique_bindings().iter().map(|b| b.use_case_field.clone()).collect();

        let mut ctor_context = Context::new();
        ctor_context.insert("generics", &self.generics());
        ctor_context.insert("name", &self.name);
        ctor_context.insert("where_clause", &self.where_clause());
        ctor_context.insert("params", &params);
        ctor_context.insert("fields", &fields);
        code.push_str(&render("file/http_adapter/ctor.rs.tera", &ctor_context));
        code
    }

    fn rpc_method_name(rpc: &GrpcRpcBinding) -> String {
        rpc.name.to_snake_case()
    }

    /// Builds the `Ok(Response::new(...))` tail: for `ai {}` use cases
    /// (flat output), each field copies straight from `response`; for CRUD
    /// use cases (wrapped output), the entity nests one level down as
    /// `response.{wrapper_field}.{field}`, wrapped in `Some(...)` since
    /// proto3 message-typed fields are always optional.
    fn gen_output_construction(rpc: &GrpcRpcBinding) -> String {
        let mut context = Context::new();
        context.insert("output_type", &rpc.output_type_name);
        match &rpc.output_wrapper_field {
            Some(wrapper_field) => {
                let entity_fields: Vec<String> = rpc.output_fields.iter().map(|(name, _)| name.clone()).collect();
                context.insert("has_wrapper", &true);
                context.insert("wrapper_field", wrapper_field);
                context.insert("entity", &rpc.output_entity);
                context.insert("entity_fields", &entity_fields);
            }
            None => {
                let output_fields: Vec<String> = rpc.output_fields.iter().map(|(name, _)| name.clone()).collect();
                context.insert("has_wrapper", &false);
                context.insert("output_fields", &output_fields);
            }
        }
        render("file/grpc_adapter/output_construction.rs.tera", &context)
    }

    fn gen_rpc_method(&self, rpc: &GrpcRpcBinding) -> String {
        let input_fields: Vec<String> = rpc.input_fields.iter().map(|(name, _)| name.clone()).collect();
        let output_construction = Self::gen_output_construction(rpc);

        let mut context = Context::new();
        context.insert("method", &Self::rpc_method_name(rpc));
        context.insert("input_type", &rpc.input_type_name);
        context.insert("output_type", &rpc.output_type_name);
        context.insert("input_fields", &input_fields);
        context.insert("field", &rpc.use_case_field);
        context.insert("output_construction", &output_construction);
        context.insert("error_type", &rpc.error_type);
        render("file/grpc_adapter/rpc_method.rs.tera", &context)
    }

    fn gen_service_impl(&self) -> String {
        let rpc_methods: Vec<String> = self.rpcs.iter().map(|rpc| self.gen_rpc_method(rpc)).collect();

        let mut context = Context::new();
        context.insert("generics", &self.generics());
        context.insert("name", &self.name);
        context.insert("where_clause", &self.where_clause());
        context.insert("rpc_methods", &rpc_methods);
        render("file/grpc_adapter/service_impl.rs.tera", &context)
    }

    pub fn write(&mut self) -> Result<(), String> {
        self.write_proto()?;

        let mut code = String::new();
        code.push_str(&self.gen_imports());
        code.push_str(&self.gen_struct());
        code.push_str(&self.gen_service_impl());

        self.rust_file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write grpc adapter file: {}", e))
    }
}
