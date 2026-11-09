use std::path::PathBuf;

use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::{
            env_template,
            file::main_file::{
                AdapterBinding, GrpcAdapterBinding, InfraBinding, InfraKind, LoadBalancerRuntime,
                MainFile, UseCaseBinding,
            },
        },
    },
    parser::{AdapterType, AllowedType, AppService, Ast, GrpcAdapterService, HttpAdapterService},
};

impl CodeGen {
    fn find_adapter<'a>(asts: &'a [Ast], name: &str) -> Option<&'a crate::parser::AdapterService> {
        asts.iter().find_map(|ast| match ast {
            Ast::Adapter(a) if a.name == name => Some(a),
            _ => None,
        })
    }

    fn default_for_env_var(name: &str, next_port: &mut u16) -> String {
        let upper = name.to_uppercase();
        if upper.contains("PORT") {
            let port = *next_port;
            *next_port += 1;
            port.to_string()
        } else if upper.contains("CORS") || upper.contains("ORIGIN") {
            // Vite's dev server — the usual browser frontend during development.
            "http://localhost:5173".to_string()
        } else if upper.contains("HOST") {
            "0.0.0.0".to_string()
        } else {
            "changeme".to_string()
        }
    }

    /// `--dev` scaffolds a `.env` with every variable the generated project
    /// actually reads (infra DB URLs plus whatever `${VAR}` placeholders each
    /// adapter's `on` uses), so `cargo run` works out of the box.
    fn write_dev_env(
        project_root: &PathBuf,
        infras: &[InfraBinding],
        adapters: &[AdapterBinding],
        grpc_adapters: &[GrpcAdapterBinding],
        load_balancers: &[LoadBalancerRuntime],
    ) -> Result<(), String> {
        let mut lines = Vec::new();
        let mut next_port: u16 = 3000;

        for infra in infras {
            match &infra.kind {
                InfraKind::Psql { .. } => {
                    let var = MainFile::db_url_env_var(&infra.infra_name);
                    lines.push(format!(
                        "{}=postgres://postgres:postgres@localhost:5432/{}",
                        var,
                        infra.entity_name.to_lowercase()
                    ));
                    lines.push(format!(
                        "{}={}",
                        MainFile::db_pool_size_env_var(&infra.infra_name),
                        crate::generator::rust_code::file::main_file::DEFAULT_DB_POOL_SIZE
                    ));
                }
                InfraKind::S3 { .. } => {
                    let (endpoint_var, access_key_var, secret_key_var) =
                        MainFile::s3_env_vars(&infra.infra_name);
                    lines.push(format!("{}=http://localhost:9000", endpoint_var));
                    lines.push(format!("{}=minioadmin", access_key_var));
                    lines.push(format!("{}=minioadmin", secret_key_var));
                }
                // `grpc_client` infras connect to a static address baked
                // directly into the generated code, not an env-var-driven
                // URL — nothing to scaffold here for them.
                InfraKind::GrpcClient { .. } => {}
                InfraKind::HttpClient { base_url, headers } => {
                    let templates = std::iter::once(base_url).chain(headers.iter().map(|(_, v)| v));
                    for var in templates.flat_map(|t| env_template::extract_vars(t)) {
                        if !lines.iter().any(|l| l.starts_with(&format!("{}=", var))) {
                            lines.push(format!("{}=changeme", var));
                        }
                    }
                }
            }
        }

        let ons: Vec<&str> = adapters
            .iter()
            .map(|a| a.on.as_str())
            .chain(adapters.iter().filter_map(|a| a.cors.as_deref()))
            .chain(grpc_adapters.iter().map(|a| a.on.as_str()))
            .chain(load_balancers.iter().map(|lb| lb.on.as_str()))
            .collect();
        for on in ons {
            for var in env_template::extract_vars(on) {
                if lines.iter().any(|l| l.starts_with(&format!("{}=", var))) {
                    continue;
                }
                let default = Self::default_for_env_var(&var, &mut next_port);
                lines.push(format!("{}={}", var, default));
            }
        }

        // `auth::UserContext` reads this to validate JWTs on any route with
        // an `auth {}` requirement. A `load_balancer {}` never has its own
        // auth bindings now — it doesn't construct any adapter locally — so
        // only bare adapters are checked.
        let has_auth = adapters.iter().any(|a| a.requires_jwt);
        if has_auth {
            lines.push("JWT_SECRET=changeme".to_string());
        }

        let content = lines.join("\n") + "\n";
        std::fs::write(project_root.join(".env"), content)
            .map_err(|e| format!("Failed to write .env: {}", e))
    }

    /// Registers an `InfraBinding` for `infra_name` (so `main.rs` connects to
    /// it and constructs its storage) unless it's already registered.
    fn register_infra_binding(
        context: &CodeGenContext,
        infra_bindings: &mut Vec<InfraBinding>,
        infra_name: &str,
        referenced_by: &str,
    ) -> Result<(), String> {
        if infra_bindings.iter().any(|i| i.infra_name == infra_name) {
            return Ok(());
        }
        if let Some(cfg) = context.find_http_client_infra(infra_name) {
            infra_bindings.push(InfraBinding {
                infra_name: infra_name.to_string(),
                entity_name: infra_name.to_string(),
                kind: InfraKind::HttpClient {
                    base_url: cfg.base_url.clone(),
                    headers: cfg.headers.clone(),
                },
            });
            return Ok(());
        }
        let entity = if context.is_blob_infra(infra_name) {
            context.find_blob_infra_entity(infra_name)
        } else {
            context.find_infra_entity(infra_name)
        }
        .ok_or_else(|| {
            format!(
                "Unknown infra '{}' referenced by {}",
                infra_name, referenced_by
            )
        })?;
        let entity_name = match entity {
            AllowedType::DefinedType(name) => name.clone(),
            _ => {
                return Err(format!("Infra '{}' entity must be a declared type", infra_name));
            }
        };
        let kind = if context.is_blob_infra(infra_name) {
            let bucket = context
                .find_blob_infra_bucket(infra_name)
                .ok_or_else(|| format!("Infra '{}' is missing its bucket", infra_name))?
                .to_string();
            InfraKind::S3 { bucket }
        } else {
            match context.find_infra_target_address(infra_name) {
                Some(address) => InfraKind::GrpcClient {
                    target_address: address.to_string(),
                },
                None => InfraKind::Psql {
                    seed: context.has_psql_seed(infra_name),
                },
            }
        };
        infra_bindings.push(InfraBinding {
            infra_name: infra_name.to_string(),
            entity_name,
            kind,
        });
        Ok(())
    }

    /// Discovers the use_cases and auth infras an adapter's routes need,
    /// registering their storage bindings along the way. Shared between a
    /// bare `app:run { Adapter }` reference and a `load_balancer {}`
    /// reference to the same adapter — both need identical wiring, they
    /// just differ in how many instances get spawned and on which address.
    fn collect_adapter_bindings(
        context: &CodeGenContext,
        http_data: &HttpAdapterService,
        infra_bindings: &mut Vec<InfraBinding>,
        use_case_bindings: &mut Vec<UseCaseBinding>,
        adapter_name: &str,
    ) -> Result<(Vec<String>, Vec<String>), String> {
        let mut use_case_names: Vec<String> = Vec::new();
        for route in &http_data.routes {
            if !use_case_names.contains(&route.use_case) {
                use_case_names.push(route.use_case.clone());
            }
        }

        // Deduped in first-occurrence order across routes, matching
        // `HttpAdapterFile::unique_auth_bindings` exactly, since that order
        // determines the adapter's generated constructor params.
        let mut auth_infra_names: Vec<String> = Vec::new();
        for route in &http_data.routes {
            if let Some(infra) = route.auth.as_ref().and_then(|a| a.permission_infra()) {
                if !auth_infra_names.iter().any(|n| n == infra) {
                    auth_infra_names.push(infra.to_string());
                }
            }
        }

        for use_case_name in &use_case_names {
            if use_case_bindings.iter().any(|u| &u.use_case_name == use_case_name) {
                continue;
            }
            let infra_names = context.find_use_case_infra(use_case_name);
            if infra_names.is_empty() && !context.is_ai_use_case_without_infra(use_case_name) {
                return Err(format!(
                    "UseCase '{}' has no known infra target (missing 'call')",
                    use_case_name
                ));
            }

            for infra_name in infra_names {
                Self::register_infra_binding(
                    context,
                    infra_bindings,
                    infra_name,
                    &format!("use_case '{}'", use_case_name),
                )?;
            }

            use_case_bindings.push(UseCaseBinding {
                use_case_name: use_case_name.clone(),
                infra_names: infra_names.to_vec(),
            });
        }

        for infra_name in &auth_infra_names {
            Self::register_infra_binding(
                context,
                infra_bindings,
                infra_name,
                &format!("adapter '{}''s auth requirement", adapter_name),
            )?;
        }

        Ok((use_case_names, auth_infra_names))
    }

    /// Same idea as `collect_adapter_bindings`, for a `grpc {}` adapter —
    /// no auth/path-param concepts to resolve, just the use_cases its rpcs
    /// reference.
    fn collect_grpc_adapter_bindings(
        context: &CodeGenContext,
        grpc_data: &GrpcAdapterService,
        infra_bindings: &mut Vec<InfraBinding>,
        use_case_bindings: &mut Vec<UseCaseBinding>,
    ) -> Result<Vec<String>, String> {
        let mut use_case_names: Vec<String> = Vec::new();
        for rpc in &grpc_data.rpcs {
            if !use_case_names.contains(&rpc.use_case) {
                use_case_names.push(rpc.use_case.clone());
            }
        }

        for use_case_name in &use_case_names {
            if use_case_bindings.iter().any(|u| &u.use_case_name == use_case_name) {
                continue;
            }
            let infra_names = context.find_use_case_infra(use_case_name);
            if infra_names.is_empty() && !context.is_ai_use_case_without_infra(use_case_name) {
                return Err(format!(
                    "UseCase '{}' has no known infra target (missing 'call')",
                    use_case_name
                ));
            }

            for infra_name in infra_names {
                Self::register_infra_binding(
                    context,
                    infra_bindings,
                    infra_name,
                    &format!("use_case '{}'", use_case_name),
                )?;
            }

            use_case_bindings.push(UseCaseBinding {
                use_case_name: use_case_name.clone(),
                infra_names: infra_names.to_vec(),
            });
        }

        Ok(use_case_names)
    }

    pub(super) fn generate_app(
        &mut self,
        context: &mut CodeGenContext,
        app: &AppService,
        asts: &[Ast],
        src_path: &PathBuf,
        package_name: &str,
        dev_mode: bool,
        project_root: &PathBuf,
    ) -> Result<(), String> {
        let mut infra_bindings: Vec<InfraBinding> = Vec::new();
        let mut use_case_bindings: Vec<UseCaseBinding> = Vec::new();
        let mut adapter_bindings: Vec<AdapterBinding> = Vec::new();
        let mut grpc_adapter_bindings: Vec<GrpcAdapterBinding> = Vec::new();
        let mut load_balancer_bindings: Vec<LoadBalancerRuntime> = Vec::new();

        for adapter_name in &app.adapters {
            let adapter = Self::find_adapter(asts, adapter_name)
                .ok_or_else(|| format!("app:run references unknown adapter '{}'", adapter_name))?;

            match &adapter.adapter_type {
                AdapterType::Http(http_data) => {
                    let on = http_data.on.clone().ok_or_else(|| {
                        format!(
                            "Adapter '{}' must declare 'on' to be started by app:run",
                            adapter_name
                        )
                    })?;

                    let (use_case_names, auth_infra_names) = Self::collect_adapter_bindings(
                        context,
                        http_data,
                        &mut infra_bindings,
                        &mut use_case_bindings,
                        adapter_name,
                    )?;

                    adapter_bindings.push(AdapterBinding {
                        adapter_name: adapter_name.clone(),
                        on,
                        use_cases: use_case_names,
                        auth_infra_names,
                        requires_jwt: http_data.routes.iter().any(|r| r.auth.is_some()),
                        cors: http_data.cors.clone(),
                    });
                }
                AdapterType::Grpc(grpc_data) => {
                    let on = grpc_data.on.clone().ok_or_else(|| {
                        format!(
                            "Adapter '{}' must declare 'on' to be started by app:run",
                            adapter_name
                        )
                    })?;

                    let use_case_names = Self::collect_grpc_adapter_bindings(
                        context,
                        grpc_data,
                        &mut infra_bindings,
                        &mut use_case_bindings,
                    )?;

                    grpc_adapter_bindings.push(GrpcAdapterBinding {
                        adapter_name: adapter_name.clone(),
                        on,
                        use_cases: use_case_names,
                    });
                }
            }
        }

        // A `load_balancer {}`'s proxy is adapter-agnostic (see
        // `templates/file/load_balancer.rs`) — its `targets` are real
        // network addresses of independently deployed processes, so this
        // binary never constructs anything for them, unlike a bare adapter
        // reference.
        for lb in &app.load_balancers {
            load_balancer_bindings.push(LoadBalancerRuntime {
                name: lb.name.clone(),
                backend_addresses: lb.targets.clone(),
                on: lb.on.clone(),
            });
        }

        context.require_git_dependency("common", "ssh://git@github.com/BH-World/common.git");
        context.require_version_dependency_with_features("tokio", "1", vec!["full"]);
        context.require_version_dependency("dotenvy", "0.15");
        if !load_balancer_bindings.is_empty() {
            // The proxy file itself (`templates/file/load_balancer.rs`)
            // uses `axum` directly to expose its own router, independent
            // of whether this app also declares a bare `http {}` adapter.
            context.require_version_dependency("axum", "0.8");
            // `stream`: request/response bodies are piped, not buffered.
            context.require_version_dependency_with_features("reqwest", "0.12", vec!["stream"]);
        }

        MainFile::new(src_path.join("main.rs"))?.write(
            package_name,
            &infra_bindings,
            &use_case_bindings,
            &adapter_bindings,
            &grpc_adapter_bindings,
            &load_balancer_bindings,
        )?;

        if dev_mode {
            Self::write_dev_env(
                project_root,
                &infra_bindings,
                &adapter_bindings,
                &grpc_adapter_bindings,
                &load_balancer_bindings,
            )?;
        }

        Ok(())
    }
}
