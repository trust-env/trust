use std::{
    fs::{File, create_dir_all},
    io::Write,
    path::PathBuf,
};

use heck::ToSnakeCase;

use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::{
            RustCode,
            file::{
                grpc_adapter_file::{GrpcAdapterFile, GrpcRpcBinding},
                http_adapter_file::{HttpAdapterFile, HttpRouteBinding, ResolvedAuth},
            },
            service::Service,
        },
    },
    parser::{
        AdapterService, AdapterType, AuthRequirement, GrpcAdapterService, GrpcRpc,
        HttpAdapterService, HttpRoute,
    },
};

impl CodeGen {
    fn empty_adapter_service(&self, name: String, folder: &PathBuf, subfolder: &str) -> Result<(), String> {
        let subfolder_path = folder.join(subfolder);
        let file_path = subfolder_path.join(format!("{}.rs", name.to_snake_case()));

        create_dir_all(&subfolder_path).map_err(|e| {
            format!(
                "error when creating the folder {:?}: {:?}",
                subfolder_path, e
            )
        })?;

        let service = Service::new(name);
        let mut file = File::create(file_path.clone())
            .map_err(|e| format!("error when creating the file {:?}: {:?}", file_path, e))?;

        file.write(service.to_rust_code().as_bytes())
            .map_err(|e| format!("error when writing to the file: {:?}", e))?;
        Ok(())
    }

    fn resolve_route_binding(
        &self,
        context: &CodeGenContext,
        route: &HttpRoute,
        type_param: String,
    ) -> Result<HttpRouteBinding, String> {
        if !context.is_known_use_case(&route.use_case) {
            return Err(format!(
                "Unknown use_case '{}' referenced in http route '{}'",
                route.use_case, route.path
            ));
        }

        // The use_case's own input/output may have been inferred (no explicit
        // type declared), in which case there's nothing to cross-check against.
        if let Some(signature) = context.find_use_case_signature(&route.use_case) {
            if let Some(input) = &route.input {
                if input != &signature.0 {
                    return Err(format!(
                        "HTTP route '{}': declared input does not match use_case '{}' input",
                        route.path, route.use_case
                    ));
                }
            }
            if let Some(output) = &route.output {
                if output != &signature.1 {
                    return Err(format!(
                        "HTTP route '{}': declared output does not match use_case '{}' output",
                        route.path, route.use_case
                    ));
                }
            }
        }

        let error_mappings = context.find_use_case_errors(&route.use_case).to_vec();
        for status_mapping in &route.status {
            let is_known_target = status_mapping.target == "_"
                || error_mappings.iter().any(|m| m.target == status_mapping.target);
            if !is_known_target {
                return Err(format!(
                    "HTTP route '{}': 'status' maps '{}', which is not one of use_case '{}''s error targets",
                    route.path, status_mapping.target, route.use_case
                ));
            }
        }

        if route.auth.is_none()
            && context
                .find_use_case_io(&route.use_case)
                .is_some_and(|io| io.requires_user)
        {
            return Err(format!(
                "HTTP route '{}': use_case '{}' is scoped to the calling user ('owner'), so the route must declare 'auth'",
                route.path, route.use_case
            ));
        }

        let (
            input_type_name,
            input_fields,
            output_type_name,
            output_entity,
            output_fields,
            output_is_list,
        ) = match context.find_use_case_io(&route.use_case) {
            Some(io) => (
                io.input_type_name.clone(),
                io.input_fields.clone(),
                io.output_type_name.clone(),
                io.output_entity.clone(),
                io.output_fields.clone(),
                io.output_is_list,
            ),
            None => (
                format!("{}Input", route.use_case),
                Vec::new(),
                format!("{}Output", route.use_case),
                String::new(),
                Vec::new(),
                false,
            ),
        };

        let raw_input = context
            .find_use_case_io(&route.use_case)
            .is_some_and(|io| io.raw_input);
        let path_params = route.path_params();
        if raw_input && !path_params.is_empty() {
            return Err(format!(
                "HTTP route '{}': use_case '{}' takes a RawRequest, so the route can't declare path parameters",
                route.path, route.use_case
            ));
        }
        for name in &path_params {
            if !input_fields.iter().any(|(n, _)| n == name) {
                return Err(format!(
                    "HTTP route '{}': path parameter ':{}' does not match any field of use_case '{}''s input",
                    route.path, name, route.use_case
                ));
            }
        }

        let requires_user = route.auth.is_some();
        let auth = match &route.auth {
            Some(AuthRequirement::Permission(req)) => {
                let entity = context.find_infra_entity(&req.infra).ok_or_else(|| {
                    format!(
                        "HTTP route '{}': auth references unknown infra '{}'",
                        route.path, req.infra
                    )
                })?;
                let entity_type = self.resolve_named_type(context, entity)?;
                for required in ["user_id", "target", "action"] {
                    if !entity_type.fields.iter().any(|f| f.name == required) {
                        return Err(format!(
                            "HTTP route '{}': auth infra '{}''s entity '{}' must have a '{}' field",
                            route.path, req.infra, entity_type.name, required
                        ));
                    }
                }
                Some(ResolvedAuth {
                    infra_name: req.infra.clone(),
                    storage_trait: format!("{}Storage", entity_type.name),
                    target: req.target.clone(),
                    action: req.action.clone(),
                })
            }
            Some(AuthRequirement::Authenticated) | None => None,
        };

        Ok(HttpRouteBinding {
            method: route.method.clone(),
            path: route.path.clone(),
            use_case_name: route.use_case.clone(),
            use_case_field: route.use_case.to_snake_case(),
            type_param,
            error_mappings,
            status_mappings: route.status.clone(),
            input_type_name,
            output_type_name,
            input_fields,
            output_entity,
            output_fields,
            output_is_list,
            path_params,
            requires_user,
            raw_input,
            auth,
        })
    }

    fn create_http_service(
        &self,
        context: &mut CodeGenContext,
        name: String,
        folder: &PathBuf,
        http_data: HttpAdapterService,
        dev_mode: bool,
    ) -> Result<(), String> {
        if http_data.routes.is_empty() {
            return self.empty_adapter_service(name, folder, "http");
        }

        context.require_git_dependency("common", "ssh://git@github.com/BH-World/common.git");
        context.require_version_dependency("axum", "0.8");

        let http_folder_path = folder.join("http");
        create_dir_all(&http_folder_path).map_err(|e| {
            format!(
                "error when creating the folder {:?}: {:?}",
                http_folder_path, e
            )
        })?;

        let mut type_params: Vec<(String, String)> = Vec::new();
        let mut bindings = Vec::new();

        for route in &http_data.routes {
            let type_param = match type_params.iter().find(|(name, _)| name == &route.use_case) {
                Some((_, param)) => param.clone(),
                None => {
                    let param = format!("C{}", type_params.len());
                    type_params.push((route.use_case.clone(), param.clone()));
                    param
                }
            };
            bindings.push(self.resolve_route_binding(context, route, type_param)?);
        }

        // The generated route file/handler function is named
        // `{use_case_name}_{http_method}` (see `HttpAdapterFile`) — keyed
        // by that pair, not by path. Two routes sharing both would collide
        // on the same generated filename and function, which only shows up
        // once the generated crate is actually compiled (a duplicate-module
        // error). Catch it here instead, with both conflicting paths named.
        for (i, a) in bindings.iter().enumerate() {
            for b in &bindings[i + 1..] {
                if a.use_case_name == b.use_case_name && a.method == b.method {
                    return Err(format!(
                        "adapter '{}': routes '{} {}' and '{} {}' both bind use_case '{}' — a use case can't be reused across two routes sharing the same HTTP method (the generated route file is named '{{use_case}}_{{method}}', not by path); give one of them its own use_case",
                        name, a.method.as_axum_fn(), a.path, b.method.as_axum_fn(), b.path, a.use_case_name
                    ));
                }
            }
        }

        if bindings.iter().any(|b| b.requires_user) {
            context.require_git_dependency("auth", "ssh://git@github.com/BH-World/auth.git");
        }

        if http_data.cors.is_some() {
            context.require_version_dependency_with_features("tower-http", "0.6", vec!["cors"]);
        }

        let mut file = HttpAdapterFile::new(
            name,
            bindings,
            dev_mode,
            http_data.cors.clone(),
            http_folder_path,
        )?;
        file.write()
    }

    /// Same cross-checks as `resolve_route_binding`, minus everything HTTP
    /// specific (path params, auth, status mappings) — gRPC errors always
    /// map to `Status::internal`, and there are no path segments to split.
    ///
    /// `pub(super)` (rather than private) because `grpc_client` storage
    /// codegen reuses this exact resolution to derive a client method's
    /// wire shape from a *different app's* `rpc` declaration, against a
    /// throwaway lookup context built over the full ast list.
    pub(super) fn resolve_grpc_binding(
        &self,
        context: &CodeGenContext,
        rpc: &GrpcRpc,
        type_param: String,
    ) -> Result<GrpcRpcBinding, String> {
        if !context.is_known_use_case(&rpc.use_case) {
            return Err(format!(
                "Unknown use_case '{}' referenced in rpc '{}'",
                rpc.use_case, rpc.name
            ));
        }

        if let Some(signature) = context.find_use_case_signature(&rpc.use_case) {
            if let Some(input) = &rpc.input {
                if input != &signature.0 {
                    return Err(format!(
                        "gRPC rpc '{}': declared input does not match use_case '{}' input",
                        rpc.name, rpc.use_case
                    ));
                }
            }
            if let Some(output) = &rpc.output {
                if output != &signature.1 {
                    return Err(format!(
                        "gRPC rpc '{}': declared output does not match use_case '{}' output",
                        rpc.name, rpc.use_case
                    ));
                }
            }
        }

        let (
            input_type_name,
            input_fields,
            output_type_name,
            output_entity,
            output_fields,
            output_wrapper_field,
        ) = match context.find_use_case_io(&rpc.use_case) {
            Some(io) => {
                if io.raw_input {
                    return Err(format!(
                        "gRPC rpc '{}': use_case '{}' takes a RawRequest, which only HTTP routes can provide",
                        rpc.name, rpc.use_case
                    ));
                }
                if io.requires_user {
                    return Err(format!(
                        "gRPC rpc '{}': use_case '{}' is scoped to the calling user ('owner'), which gRPC adapters can't authenticate yet",
                        rpc.name, rpc.use_case
                    ));
                }
                if io.output_is_list {
                    return Err(format!(
                        "gRPC rpc '{}': use_case '{}' returns a list, which isn't supported over gRPC yet",
                        rpc.name, rpc.use_case
                    ));
                }
                (
                    io.input_type_name.clone(),
                    io.input_fields.clone(),
                    io.output_type_name.clone(),
                    io.output_entity.clone(),
                    io.output_fields.clone(),
                    io.output_wrapper_field.clone(),
                )
            }
            None => (
                format!("{}Input", rpc.use_case),
                Vec::new(),
                format!("{}Output", rpc.use_case),
                String::new(),
                Vec::new(),
                None,
            ),
        };

        Ok(GrpcRpcBinding {
            name: rpc.name.clone(),
            use_case_name: rpc.use_case.clone(),
            use_case_field: rpc.use_case.to_snake_case(),
            type_param,
            input_type_name,
            input_fields,
            output_type_name,
            output_entity,
            output_wrapper_field,
            output_fields,
            error_type: format!("{}Error", rpc.use_case),
        })
    }

    fn create_grpc_service(
        &self,
        context: &mut CodeGenContext,
        name: String,
        folder: &PathBuf,
        proto_dir: &PathBuf,
        grpc_data: GrpcAdapterService,
    ) -> Result<(), String> {
        if grpc_data.rpcs.is_empty() {
            return self.empty_adapter_service(name, folder, "grpc");
        }

        context.require_git_dependency("common", "ssh://git@github.com/BH-World/common.git");
        context.require_version_dependency("tonic", "0.12");
        context.require_version_dependency("prost", "0.13");
        context.require_build_dependency("tonic-build", "0.12");
        context.require_build_dependency("protoc-bin-vendored", "3");

        let grpc_folder_path = folder.join("grpc");
        create_dir_all(&grpc_folder_path).map_err(|e| {
            format!("error when creating the folder {:?}: {:?}", grpc_folder_path, e)
        })?;
        create_dir_all(proto_dir)
            .map_err(|e| format!("error when creating the folder {:?}: {:?}", proto_dir, e))?;

        let mut type_params: Vec<(String, String)> = Vec::new();
        let mut bindings = Vec::new();

        for rpc in &grpc_data.rpcs {
            let type_param = match type_params.iter().find(|(name, _)| name == &rpc.use_case) {
                Some((_, param)) => param.clone(),
                None => {
                    let param = format!("C{}", type_params.len());
                    type_params.push((rpc.use_case.clone(), param.clone()));
                    param
                }
            };
            bindings.push(self.resolve_grpc_binding(context, rpc, type_param)?);
        }

        let mut file = GrpcAdapterFile::new(name, bindings, proto_dir.clone(), grpc_folder_path)?;
        context.register_grpc_proto_path(file.proto_path().clone());
        file.write()
    }

    pub(super) fn generate_adapter(
        &mut self,
        context: &mut CodeGenContext,
        adapter: AdapterService,
        folder: PathBuf,
        dev_mode: bool,
        proto_dir: &PathBuf,
    ) -> Result<(), String> {
        match adapter.adapter_type {
            AdapterType::Http(http_data) => {
                self.create_http_service(context, adapter.name.clone(), &folder, http_data, dev_mode)?;
                context.register_adapter_http_module(adapter.name.to_snake_case());
            }
            AdapterType::Grpc(grpc_data) => {
                self.create_grpc_service(context, adapter.name.clone(), &folder, proto_dir, grpc_data)?;
                context.register_adapter_grpc_module(adapter.name.to_snake_case());
            }
        }
        Ok(())
    }
}
