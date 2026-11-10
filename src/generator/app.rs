use std::path::PathBuf;

use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::{
            file::mcp_adapter_file::McpAuth,
            env_template,
            file::main_file::{
                AdapterBinding, GrpcAdapterBinding, InfraBinding, InfraKind, LoadBalancerRuntime,
                MainFile, ScheduleJobRuntime, UseCaseBinding,
            },
        },
    },
    parser::{
        AdapterType, AllowedType, AppService, Ast, GrpcAdapterService, HttpAdapterService,
        ScheduleDeclaration,
    },
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
        #[derive(serde::Serialize)]
        struct EnvEntry {
            name: String,
            value: String,
        }
        let mut entries: Vec<EnvEntry> = Vec::new();
        let push = |entries: &mut Vec<EnvEntry>, name: String, value: String| {
            if !entries.iter().any(|e| e.name == name) {
                entries.push(EnvEntry { name, value });
            }
        };
        let mut next_port: u16 = 3000;

        for infra in infras {
            match &infra.kind {
                // One database for the app: DATABASE_URL (each infra can
                // still be pointed elsewhere with <INFRA>_DB_URL).
                InfraKind::Psql { database: None, .. } => {
                    push(
                        &mut entries,
                        "DATABASE_URL".to_string(),
                        format!("postgres://postgres:postgres@localhost:5432/{}", project_root
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("app")),
                    );
                }
                InfraKind::Psql { database: Some((url, _)), .. } => {
                    for var in env_template::extract_vars(url) {
                        let value = env_template::var_default(url, &var)
                            .unwrap_or_else(|| "postgres://postgres:postgres@localhost:5432/app".to_string());
                        push(&mut entries, var, value);
                    }
                }
                InfraKind::S3 { .. } => {
                    let (endpoint_var, access_key_var, secret_key_var) =
                        MainFile::s3_env_vars(&infra.infra_name);
                    push(&mut entries, endpoint_var, "http://localhost:9000".to_string());
                    push(&mut entries, access_key_var, "minioadmin".to_string());
                    push(&mut entries, secret_key_var, "minioadmin".to_string());
                }
                // `grpc_client` infras connect to a static address baked
                // directly into the generated code, not an env-var-driven
                // URL — nothing to scaffold here for them.
                InfraKind::GrpcClient { .. } => {}
                InfraKind::HttpClient { base_url, headers } => {
                    let templates = std::iter::once(base_url).chain(headers.iter().map(|(_, v)| v));
                    for template in templates {
                        for var in env_template::extract_vars(template) {
                            let value = env_template::var_default(template, &var).unwrap_or_else(|| "changeme".to_string());
                            push(&mut entries, var, value);
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
                if entries.iter().any(|e| e.name == var) {
                    continue;
                }
                let default = env_template::var_default(on, &var)
                    .unwrap_or_else(|| Self::default_for_env_var(&var, &mut next_port));
                push(&mut entries, var, default);
            }
        }

        // `auth::UserContext` reads this to validate JWTs on any route with
        // an `auth {}` requirement. A `load_balancer {}` never has its own
        // auth bindings now — it doesn't construct any adapter locally — so
        // only bare adapters are checked.
        let has_auth = adapters.iter().any(|a| a.requires_jwt);
        if has_auth {
            entries.push(EnvEntry { name: "JWT_SECRET".to_string(), value: "changeme".to_string() });
        }

        let mut context = tera::Context::new();
        context.insert("entries", &entries);
        let content = crate::generator::rust_code::render::render("env/dotenv.tera", &context);
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
                    foreign_keys: context.has_psql_foreign_keys(infra_name),
                    database: context.find_psql_database(infra_name).map(|d| (d.url.clone(), d.pool)),
                },
            }
        };
        let is_psql = matches!(kind, InfraKind::Psql { .. });
        infra_bindings.push(InfraBinding {
            infra_name: infra_name.to_string(),
            entity_name,
            kind,
        });
        // A foreign key needs the referenced table: connect (and migrate)
        // that infra too.
        if is_psql {
            for referenced in context.find_psql_references(infra_name).to_vec() {
                Self::register_infra_binding(
                    context,
                    infra_bindings,
                    &referenced,
                    &format!("infra '{}''s references", infra_name),
                )?;
            }
        }
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
                use_case_deps: Vec::new(),
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
                use_case_deps: Vec::new(),
            });
        }

        Ok(use_case_names)
    }

    fn find_schedule<'a>(asts: &'a [Ast], name: &str) -> Option<&'a ScheduleDeclaration> {
        asts.iter().find_map(|ast| match ast {
            Ast::Schedule(s) if s.name == name => Some(s),
            _ => None,
        })
    }

    /// Checks a `schedule {}` (cron syntax, time zones, use cases) and
    /// registers the use cases it runs, like an adapter's routes.
    fn collect_schedule_bindings(
        context: &CodeGenContext,
        schedule: &ScheduleDeclaration,
        infra_bindings: &mut Vec<InfraBinding>,
        use_case_bindings: &mut Vec<UseCaseBinding>,
        jobs: &mut Vec<ScheduleJobRuntime>,
    ) -> Result<(), String> {
        for job in &schedule.jobs {
            let cron = crate::cron::parse(&job.cron)
                .map_err(|e| format!("schedule '{}': {}", schedule.name, e))?;
            let tz = job.tz.clone().or_else(|| schedule.tz.clone()).unwrap_or_else(|| "UTC".to_string());
            tz.parse::<chrono_tz::Tz>().map_err(|_| {
                format!("schedule '{}': unknown time zone '{}' (use an IANA name such as \"Europe/Paris\")", schedule.name, tz)
            })?;
            if !context.is_known_use_case(&job.use_case) {
                return Err(format!("schedule '{}' runs unknown use_case '{}'", schedule.name, job.use_case));
            }
            if context.find_use_case_io(&job.use_case).is_some_and(|io| io.raw_input) {
                return Err(format!(
                    "schedule '{}': use_case '{}' takes a RawRequest, which a schedule can't provide",
                    schedule.name, job.use_case
                ));
            }
            if !use_case_bindings.iter().any(|u| u.use_case_name == job.use_case) {
                let infra_names = context.find_use_case_infra(&job.use_case);
                if infra_names.is_empty() && !context.is_ai_use_case_without_infra(&job.use_case) {
                    return Err(format!(
                        "UseCase '{}' has no known infra target (missing 'call')",
                        job.use_case
                    ));
                }
                for infra_name in infra_names {
                    Self::register_infra_binding(
                        context,
                        infra_bindings,
                        infra_name,
                        &format!("use_case '{}'", job.use_case),
                    )?;
                }
                use_case_bindings.push(UseCaseBinding {
                    use_case_name: job.use_case.clone(),
                    infra_names: infra_names.to_vec(),
                    use_case_deps: Vec::new(),
                });
            }
            // A literal input: field names checked against the input type
            // and written as they travel (json_case).
            let input = match &job.input {
                None => None,
                Some(value) => {
                    let io = context
                        .find_use_case_io(&job.use_case)
                        .ok_or_else(|| format!("schedule '{}': use_case '{}' has no resolved input", schedule.name, job.use_case))?;
                    let serde_json::Value::Object(fields) = value else {
                        return Err(format!("schedule '{}': the input of '{}' must be {{ ... }}", schedule.name, job.use_case));
                    };
                    let mut wire = serde_json::Map::new();
                    for (key, field_value) in fields {
                        if !io.input_type.fields.iter().any(|f| &f.name == key) {
                            return Err(format!(
                                "schedule '{}': '{}' has no input field '{}'",
                                schedule.name, job.use_case, key
                            ));
                        }
                        wire.insert(crate::generator::schema::wire_name(key, context.json_camel()), field_value.clone());
                    }
                    Some((io.input_type_name.clone(), serde_json::Value::Object(wire).to_string()))
                }
            };
            jobs.push(ScheduleJobRuntime {
                input,
                name: format!("{}.{}", schedule.name, job.use_case),
                cron_text: job.cron.clone(),
                cron,
                tz,
                use_case: job.use_case.clone(),
                user: schedule.user,
            });
        }
        Ok(())
    }

    /// Binds the use cases `code {}` bodies run (and their infras), then
    /// orders every binding so a use case is built after those it runs.
    fn resolve_use_case_dependencies(
        context: &CodeGenContext,
        infra_bindings: &mut Vec<InfraBinding>,
        use_case_bindings: &mut Vec<UseCaseBinding>,
    ) -> Result<(), String> {
        let mut i = 0;
        while i < use_case_bindings.len() {
            let deps = context.find_use_case_deps(&use_case_bindings[i].use_case_name).to_vec();
            for dep in &deps {
                if !context.is_known_use_case(dep) {
                    return Err(format!(
                        "UseCase '{}': 'code' runs unknown use_case '{}'",
                        use_case_bindings[i].use_case_name, dep
                    ));
                }
                if use_case_bindings.iter().any(|u| &u.use_case_name == dep) {
                    continue;
                }
                let infra_names = context.find_use_case_infra(dep).to_vec();
                for infra_name in &infra_names {
                    Self::register_infra_binding(context, infra_bindings, infra_name, &format!("use_case '{}'", dep))?;
                }
                use_case_bindings.push(UseCaseBinding {
                    use_case_name: dep.clone(),
                    infra_names,
                    use_case_deps: Vec::new(),
                });
            }
            use_case_bindings[i].use_case_deps = deps;
            i += 1;
        }

        // Depth-first: dependencies first; a cycle is an error.
        fn visit(
            name: &str,
            bindings: &[UseCaseBinding],
            done: &mut Vec<String>,
            path: &mut Vec<String>,
        ) -> Result<(), String> {
            if done.iter().any(|d| d == name) {
                return Ok(());
            }
            if path.iter().any(|p| p == name) {
                path.push(name.to_string());
                return Err(format!("use cases run each other in a cycle: {}", path.join(" -> ")));
            }
            path.push(name.to_string());
            if let Some(binding) = bindings.iter().find(|b| b.use_case_name == name) {
                for dep in &binding.use_case_deps {
                    visit(dep, bindings, done, path)?;
                }
            }
            path.pop();
            done.push(name.to_string());
            Ok(())
        }
        let mut order = Vec::new();
        for binding in use_case_bindings.iter() {
            visit(&binding.use_case_name, use_case_bindings, &mut order, &mut Vec::new())?;
        }
        use_case_bindings.sort_by_key(|b| order.iter().position(|o| o == &b.use_case_name));
        Ok(())
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

        let mut schedule_jobs: Vec<ScheduleJobRuntime> = Vec::new();
        for adapter_name in &app.adapters {
            if let Some(schedule) = Self::find_schedule(asts, adapter_name) {
                Self::collect_schedule_bindings(
                    context,
                    schedule,
                    &mut infra_bindings,
                    &mut use_case_bindings,
                    &mut schedule_jobs,
                )?;
                continue;
            }
            let adapter = Self::find_adapter(asts, adapter_name)
                .ok_or_else(|| format!("app:run references unknown adapter or schedule '{}'", adapter_name))?;

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
                        requires_jwt: http_data.routes.iter().any(|r| r.auth.as_ref().is_some_and(|a| a.uses_jwt()))
                            || http_data.events.as_ref().is_some_and(|e| e.requires_user),
                        cors: http_data.cors.clone(),
                        mcp_stdio_user: None,
                    });
                }
                AdapterType::Mcp(mcp_data) => {
                    let on = mcp_data.on.clone().ok_or_else(|| {
                        format!(
                            "Adapter '{}' must declare 'on' to be started by app:run",
                            adapter_name
                        )
                    })?;
                    let file = Self::mcp_adapter_file(context, adapter_name, mcp_data)?;
                    let use_case_names = file.constructor_use_cases();
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
                                &mut infra_bindings,
                                infra_name,
                                &format!("use_case '{}'", use_case_name),
                            )?;
                        }
                        use_case_bindings.push(UseCaseBinding {
                            use_case_name: use_case_name.clone(),
                            infra_names: infra_names.to_vec(),
                            use_case_deps: Vec::new(),
                        });
                    }
                    // `<binary> mcp` (stdio) acts as the secret's user, or
                    // `MCP_USER_ID`.
                    let stdio_user = match &file.auth {
                        McpAuth::Secret { user, .. } => *user,
                        _ => 0,
                    };
                    adapter_bindings.push(AdapterBinding {
                        adapter_name: adapter_name.clone(),
                        on,
                        use_cases: use_case_names,
                        auth_infra_names: Vec::new(),
                        requires_jwt: matches!(file.auth, McpAuth::Jwt),
                        cors: None,
                        mcp_stdio_user: Some(stdio_user),
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
        // Structured logs: `RUST_LOG` filters, `LOG_FORMAT=json` for JSON lines.
        context.require_version_dependency("tracing", "0.1");
        context.require_version_dependency_with_features(
            "tracing-subscriber",
            "0.3",
            vec!["env-filter", "json"],
        );
        // `<binary> token <user_id> [days]` mints long-lived service tokens.
        if adapter_bindings.iter().any(|a| a.requires_jwt) {
            context.require_version_dependency("jsonwebtoken", "9");
            context.require_version_dependency("serde_json", "1");
        }
        if schedule_jobs.iter().any(|j| j.input.is_some()) {
            context.require_version_dependency("serde_json", "1");
        }
        if !schedule_jobs.is_empty() {
            context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
            context.require_version_dependency("chrono-tz", "0.10");
            context.require_version_dependency("tracing", "0.1");
        }
        if !load_balancer_bindings.is_empty() {
            // The proxy file itself (`templates/file/load_balancer.rs`)
            // uses `axum` directly to expose its own router, independent
            // of whether this app also declares a bare `http {}` adapter.
            context.require_version_dependency("axum", "0.8");
            // `stream`: request/response bodies are piped, not buffered.
            context.require_version_dependency_with_features("reqwest", "0.12", vec!["stream"]);
        }

        Self::resolve_use_case_dependencies(context, &mut infra_bindings, &mut use_case_bindings)?;

        MainFile::new(src_path.join("main.rs"))?.write(
            package_name,
            &infra_bindings,
            &use_case_bindings,
            &adapter_bindings,
            &grpc_adapter_bindings,
            &load_balancer_bindings,
            &schedule_jobs,
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
