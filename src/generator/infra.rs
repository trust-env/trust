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
        tera_context.insert("notify", &false);
        if let Some(notify) = &cfg.notify {
            Self::notify_context(context, infra_name, notify, &mut tera_context)?;
        }
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

    /// The template values of a notify-mode client (`notifications {}`, a
    /// bot's channel).
    fn notify_context(
        context: &mut CodeGenContext,
        infra_name: &str,
        notify: &crate::parser::NotifyOptions,
        tera_context: &mut tera::Context,
    ) -> Result<(), String> {
        use crate::generator::rust_code::env_template::to_rust_expr_lenient;
        #[derive(serde::Serialize)]
        struct ChannelView {
            name: String,
            label: String,
            kind: String,
            token: String,
            chat: String,
            api_url: String,
            url: String,
            format: String,
        }
        let setting = |c: &crate::parser::ChannelDeclaration, key: &str, default: &str| {
            let value = c.settings.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_else(|| default.to_string());
            to_rust_expr_lenient(&value)
        };
        let channels: Vec<ChannelView> = notify
            .channels
            .iter()
            .map(|c| ChannelView {
                name: c.name.clone(),
                label: c.name.to_snake_case(),
                kind: c.kind.clone(),
                token: setting(c, "token", ""),
                chat: setting(c, "chat", ""),
                api_url: setting(c, "api_url", "https://api.telegram.org"),
                url: setting(c, "url", ""),
                format: c.settings.iter().find(|(k, _)| k == "format").map(|(_, v)| v.clone()).unwrap_or_else(|| "json".to_string()),
            })
            .collect();
        let quote = |names: &[String]| names.iter().map(|n| format!("{:?}", n)).collect::<Vec<_>>().join(", ");
        let routing: Vec<(String, String)> = notify.routing.iter().map(|(kind, names)| (format!("{:?}", kind), quote(names))).collect();
        tera_context.insert("notify", &true);
        tera_context.insert("channels", &channels);
        tera_context.insert("default_channels", &quote(&notify.default));
        tera_context.insert("fallback", &notify.fallback);
        tera_context.insert("routing", &routing);
        tera_context.insert("record", &false);
        if let Some(record) = &notify.record {
            let entity_name = match context.find_infra_entity(record) {
                Some(crate::parser::AllowedType::DefinedType(name)) => name.clone(),
                _ => return Err(format!("notifications '{}': '{}' is not a psql infra", infra_name, record)),
            };
            let entity = context
                .find_type(&entity_name)
                .cloned()
                .ok_or_else(|| format!("notifications '{}': unknown entity '{}'", infra_name, entity_name))?;
            let mut required = vec!["id", "kind", "at", "channel", "text", "status", "error", "external_id"];
            if let Some(owner) = &notify.record_owner {
                required.push(owner.as_str());
            }
            for field in &required {
                if !entity.fields.iter().any(|f| f.name == *field) {
                    return Err(format!(
                        "notifications '{}': entity '{}' needs a field '{}' (kind, at, channel, text, status, error, external_id and the owner)",
                        infra_name, entity_name, field
                    ));
                }
            }
            if let Some(extra) = entity.fields.iter().find(|f| !required.contains(&f.name.as_str()) && !f.field_type.is_option()) {
                return Err(format!(
                    "notifications '{}': entity '{}' field '{}' must be optional (Trust only fills {})",
                    infra_name,
                    entity_name,
                    extra.name,
                    required.join(", ")
                ));
            }
            let camel = context.json_camel();
            let key = |name: &str| format!("{:?}", crate::generator::schema::wire_name(name, camel));
            tera_context.insert("record", &true);
            tera_context.insert("record_entity", &entity_name);
            tera_context.insert("record_owner_key", &notify.record_owner.as_deref().map(key));
            tera_context.insert("external_id_key", &key("external_id"));
        }
        context.needs_notify = true;
        context.require_version_dependency("tracing", "0.1");
        context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
        Ok(())
    }
}
