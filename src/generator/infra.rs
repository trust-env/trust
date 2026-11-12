mod storage;

use heck::ToSnakeCase;

use std::path::{Path, PathBuf};

use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::service::Service,
    },
    parser::{
        Ast, InfraService,
        InfraServiceType::{HttpClient, Storage},
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
            Storage(s) => self.gen_storage(context, &service.name, s, folder, full_asts, proto_dir),
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

        let cfg = context
            .find_http_client_infra(infra_name)
            .cloned()
            .ok_or_else(|| format!("Unknown http_client infra '{}'", infra_name))?;
        let mut tera_context = tera::Context::new();
        tera_context.insert("retry", &cfg.retry);
        tera_context.insert("timeout_secs", &cfg.timeout_secs);
        tera_context.insert("infra", infra_name);
        tera_context.insert("name", &Self::http_client_struct_name(infra_name));
        tera_context.insert("ics", &cfg.ics.is_some());
        if let Some(ics) = &cfg.ics {
            ics.timezone_fallback.parse::<chrono_tz::Tz>().map_err(|_| {
                format!("infra '{}': unknown timezone_fallback '{}' (use an IANA name such as \"Europe/Paris\")", infra_name, ics.timezone_fallback)
            })?;
            tera_context.insert("ics_fallback", &ics.timezone_fallback);
            tera_context.insert("ics_all_day", &ics.include_all_day);
            context.needs_ics = true;
            context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
            context.require_version_dependency("chrono-tz", "0.10");
            context.require_version_dependency("rrule", "0.14");
            context.require_version_dependency("tracing", "0.1");
        }
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
