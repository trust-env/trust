mod storage;

use heck::ToSnakeCase;

use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::{RustCode, service::Service},
    },
    parser::{
        Ast, InfraService,
        InfraServiceType::{HttpClient, Storage},
        PsqlInfraService,
        StorageType::Psql,
    },
};

struct CodeInfraContext<'a> {
    folder: &'a PathBuf,
    service: &'a Service,
}

impl CodeGen {
    pub(super) fn generate_infra(
        &mut self,
        context: &mut CodeGenContext,
        infra: InfraService,
        folder: PathBuf,
        full_asts: &[Ast],
        proto_dir: &PathBuf,
    ) -> Result<(), String> {
        let service = Service::new(infra.name);

        match infra.service_type {
            Storage(s) => self.gen_storage(context, s, folder, full_asts, proto_dir),
            HttpClient(_) => self.gen_http_client(context, &service.name, &folder),
        }
    }

    /// The struct name an `http_client` infra generates, e.g. `PaymentApi`
    /// → `PaymentApiClient`.
    pub(super) fn http_client_struct_name(infra_name: &str) -> String {
        format!("{}Client", infra_name)
    }

    fn gen_http_client(
        &mut self,
        context: &mut CodeGenContext,
        infra_name: &str,
        folder: &Path,
    ) -> Result<(), String> {
        let dir = folder.join("http_client");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create http_client folder: {}", e))?;
        let module = infra_name.to_snake_case();

        let mut tera_context = tera::Context::new();
        tera_context.insert("infra", infra_name);
        tera_context.insert("name", &Self::http_client_struct_name(infra_name));
        let code = crate::generator::rust_code::render::render(
            "file/http_client/client.rs.tera",
            &tera_context,
        );
        std::fs::write(dir.join(format!("{}.rs", module)), code)
            .map_err(|e| format!("Failed to write http_client '{}': {}", infra_name, e))?;

        context.require_version_dependency_with_features("reqwest", "0.12", vec!["json"]);
        context.require_version_dependency("serde_json", "1");
        context.require_version_dependency_with_features("serde", "1", vec!["derive"]);
        context.register_http_client_module(module);
        Ok(())
    }
}
