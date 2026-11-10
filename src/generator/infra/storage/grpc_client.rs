use std::{fs::create_dir_all, path::PathBuf};

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::file::grpc_client_storage_file::GrpcClientStorageFile},
    parser::{AdapterType, AllowedType, Ast, GrpcClientInfraService},
};

impl CodeGen {
    fn find_target_adapter<'a>(asts: &'a [Ast], name: &str) -> Option<&'a crate::parser::AdapterService> {
        asts.iter().find_map(|ast| match ast {
            Ast::Adapter(a) if a.name == name => Some(a),
            _ => None,
        })
    }

    /// Generates a `Grpc{Entity}Client` implementing the same generic
    /// `Storage<T, E>` trait `psql` storage implements, backed by a real
    /// gRPC call to `cfg.service` — a `grpc {}` adapter declared elsewhere
    /// in the file, normally in a *different* app. Because that adapter
    /// lives in a different app's partitioned `Vec<Ast>` after
    /// `workspace::partition_by_app`, its rpc/use_case shape is resolved
    /// against `full_asts` (the original, unfiltered list) via a throwaway
    /// lookup context — never against `context` (this crate's own, which
    /// only knows about declarations reachable from this app).
    pub(super) fn gen_grpc_client_storage(
        &mut self,
        context: &mut CodeGenContext,
        cfg: GrpcClientInfraService,
        folder: PathBuf,
        full_asts: &[Ast],
        proto_dir: &PathBuf,
    ) -> Result<(), String> {
        let grpc_client_folder_path = folder.join("grpc_client");
        create_dir_all(&grpc_client_folder_path)
            .map_err(|e| format!("Failed to create grpc_client folder: {}", e))?;
        create_dir_all(proto_dir)
            .map_err(|e| format!("Failed to create proto folder: {}", e))?;

        let entity_name = match &cfg.entity {
            AllowedType::DefinedType(name) => name.clone(),
            _ => return Err("grpc_client 'entity' must be a declared type".to_string()),
        };
        let entity_type = context
            .find_type(&entity_name)
            .cloned()
            .ok_or_else(|| format!("grpc_client: entity type '{}' not found in context", entity_name))?;

        let adapter = Self::find_target_adapter(full_asts, &cfg.service)
            .ok_or_else(|| format!("grpc_client: unknown target service '{}'", cfg.service))?;
        let grpc_data = match &adapter.adapter_type {
            AdapterType::Grpc(g) => g,
            AdapterType::Http(_) | AdapterType::Mcp(_) => {
                return Err(format!("grpc_client: target service '{}' is not a grpc {{}} adapter", cfg.service));
            }
        };

        // Read-only: registers the target's use_case IO signatures (and
        // everything else `resolve_grpc_binding` needs) without writing
        // any file, so the target rpc's wire shape can be resolved exactly
        // like the server side resolved it — even though that use_case
        // isn't reachable from this app's own `context`.
        let lookup_context = self.build_lookup_context(full_asts)?;

        let mut bindings = Vec::new();
        for mapping in &cfg.methods {
            if mapping.storage_method != "find_one" {
                return Err(format!(
                    "grpc_client '{}': storage method '{}' isn't supported yet — only 'find_one' is implemented",
                    cfg.service, mapping.storage_method
                ));
            }
            let rpc = grpc_data.rpcs.iter().find(|r| r.name == mapping.rpc).ok_or_else(|| {
                format!("grpc_client: service '{}' has no rpc named '{}'", cfg.service, mapping.rpc)
            })?;
            let binding = self.resolve_grpc_binding(&lookup_context, rpc, "C0".to_string())?;
            bindings.push((mapping.storage_method.clone(), binding));
        }

        context.require_git_dependency("common", "ssh://git@github.com/BH-World/common.git");
        context.require_version_dependency("tonic", "0.12");
        context.require_version_dependency("prost", "0.13");
        context.require_build_dependency("tonic-build", "0.12");
        context.require_build_dependency("protoc-bin-vendored", "3");

        let mut file = GrpcClientStorageFile::new(
            entity_type,
            cfg.service.clone(),
            cfg.on.clone(),
            bindings,
            proto_dir.clone(),
            grpc_client_folder_path,
        )?;
        context.register_grpc_proto_path(file.proto_path().clone());
        context.register_grpc_client_entity_module(file.module_name());
        file.write()
    }
}
