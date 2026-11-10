use std::{fs::File, io::Write, path::PathBuf};

use heck::{ToShoutySnakeCase, ToSnakeCase};

use crate::generator::rust_code::{
    env_template, imports::use_statement, migration, render::render,
};
use tera::Context;

/// Connections each `psql` infra's pool may open when its
/// `<INFRA>_DB_POOL_SIZE` env var is unset. Postgres allows 100 connections
/// by default, shared by every infra of every running app.
pub const DEFAULT_DB_POOL_SIZE: u32 = 16;

/// Distinguishes how `gen_infra` connects a `psql` infra (an env-var-driven
/// DB URL), a `grpc_client` infra (a static address baked directly into the
/// `.tr` file, no env var), and an `s3` infra (three env-var-driven
/// credentials plus a literal bucket name) — see
/// `CodeGenContext::find_infra_target_address`/`is_blob_infra`, the source
/// of truth `register_infra_binding` checks to pick a kind.
pub enum InfraKind {
    /// `seed`: the infra declares a `seed:` file, applied after its table.
    Psql {
        seed: bool,
        /// `migrations/<table>.fk.sql` exists: run once every table does.
        foreign_keys: bool,
    },
    GrpcClient {
        target_address: String,
    },
    S3 {
        bucket: String,
    },
    /// An `http_client {}` infra: `base_url` and header values are
    /// `${VAR}` templates resolved at startup.
    HttpClient {
        base_url: String,
        headers: Vec<(String, String)>,
    },
}

pub struct InfraBinding {
    pub infra_name: String,
    pub entity_name: String,
    pub kind: InfraKind,
}

pub struct UseCaseBinding {
    pub use_case_name: String,
    /// Infra targets this use case calls, in declaration order — the
    /// constructor takes one storage argument per entry, in this order.
    pub infra_names: Vec<String>,
}

pub struct AdapterBinding {
    pub adapter_name: String,
    pub on: String,
    pub use_cases: Vec<String>,
    /// Infra targets referenced by this adapter's routes' `auth {}` blocks,
    /// deduped in first-occurrence order — the adapter's constructor takes
    /// one storage argument per entry, after the use case arguments, in
    /// this order (matches `HttpAdapterFile::unique_auth_bindings`).
    pub auth_infra_names: Vec<String>,
    /// Any route declares `auth` (either form) — the JWT secret is read.
    pub requires_jwt: bool,
    /// The adapter's `cors:` template, scanned for `${VAR}`s in `--dev`.
    pub cors: Option<String>,
    /// An `mcp {}` adapter: `<binary> mcp` serves it over stdio, acting as
    /// this user (unless `MCP_USER_ID` says otherwise).
    pub mcp_stdio_user: Option<i64>,
}

pub struct GrpcAdapterBinding {
    pub adapter_name: String,
    pub on: String,
    pub use_cases: Vec<String>,
}

/// A `load_balancer {}` from `app:run`: an adapter-agnostic round-robin
/// reverse proxy, exposed on `on`, forwarding to `backend_addresses` — real
/// network addresses of independently deployed processes. This binary
/// never constructs anything for them; it only proxies.
pub struct LoadBalancerRuntime {
    pub name: String,
    pub backend_addresses: Vec<String>,
    pub on: String,
}

/// One `"<cron>" => UseCase` of a `schedule {}` listed in the app.
pub struct ScheduleJobRuntime {
    /// `Schedule.UseCase`, used in logs.
    pub name: String,
    pub cron_text: String,
    pub cron: crate::cron::CronMask,
    /// IANA time zone, already validated.
    pub tz: String,
    pub use_case: String,
    /// The user id runs act as.
    pub user: i64,
}

pub struct MainFile {
    file: File,
}

impl MainFile {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        Ok(MainFile {
            file: File::create(&path)
                .map_err(|e| format!("Failed to create main.rs: {}", e))?,
        })
    }

    pub fn db_url_env_var(infra_name: &str) -> String {
        format!("{}_DB_URL", infra_name.to_shouty_snake_case())
    }

    /// Optional env var sizing a `psql` infra's connection pool.
    pub fn db_pool_size_env_var(infra_name: &str) -> String {
        format!("{}_DB_POOL_SIZE", infra_name.to_shouty_snake_case())
    }

    /// `(endpoint, access_key, secret_key)` env var names for an `s3 {}`
    /// infra — same "derived from the infra name" precedent as
    /// `db_url_env_var`, since endpoint/credentials are never written in
    /// `.tr` source (see `S3InfraService`'s doc comment).
    pub fn s3_env_vars(infra_name: &str) -> (String, String, String) {
        let upper = infra_name.to_shouty_snake_case();
        (format!("{}_S3_ENDPOINT", upper), format!("{}_S3_ACCESS_KEY", upper), format!("{}_S3_SECRET_KEY", upper))
    }

    fn gen_infra(infra: &InfraBinding) -> String {
        // The constructed storage variable is always named after the
        // *infra*, not the entity: `register_infra_binding` dedupes by
        // infra name (unique by construction), but two different infras
        // — e.g. a `psql` metadata infra and an `s3` blob infra — can
        // legitimately target the *same* entity (see `S3InfraService`'s
        // doc comment). Naming the variable after the entity would give
        // both a `let media_storage = ...`, silently shadowing one with
        // the other.
        let infra_var = infra.infra_name.to_snake_case();
        match &infra.kind {
            InfraKind::Psql { seed, .. } => {
                let db_var = infra.infra_name.to_snake_case();
                let env_var = Self::db_url_env_var(&infra.infra_name);

                let mut migrations = String::new();
                let mut files = vec![migration::migration_file_name(&infra.entity_name)];
                if *seed {
                    files.push(migration::seed_file_name(&infra.entity_name));
                }
                for file in files {
                    let mut context = Context::new();
                    context.insert("file", &file);
                    context.insert("db_var", &db_var);
                    migrations.push_str(&render("file/main/migration.rs.tera", &context));
                }

                let mut context = Context::new();
                context.insert("db_var", &db_var);
                context.insert("env_var_literal", &format!("{:?}", env_var));
                context.insert("env_var", &env_var);
                let pool_env_var = Self::db_pool_size_env_var(&infra.infra_name);
                context.insert("pool_env_var_literal", &format!("{:?}", pool_env_var));
                context.insert("pool_env_var", &pool_env_var);
                context.insert("default_pool_size", &DEFAULT_DB_POOL_SIZE);
                context.insert("entity_var", &infra_var);
                context.insert("entity", &infra.entity_name);
                context.insert("migrations", &migrations);
                render("file/main/infra.rs.tera", &context)
            }
            InfraKind::GrpcClient { target_address } => {
                let mut context = Context::new();
                context.insert("entity_var", &infra_var);
                context.insert("entity", &infra.entity_name);
                context.insert("target_address_literal", &format!("{:?}", target_address));
                render("file/main/grpc_client_infra.rs.tera", &context)
            }
            InfraKind::S3 { bucket } => {
                let (endpoint_var, access_key_var, secret_key_var) = Self::s3_env_vars(&infra.infra_name);

                let mut context = Context::new();
                context.insert("entity_var", &infra_var);
                context.insert("entity", &infra.entity_name);
                context.insert("endpoint_var_literal", &format!("{:?}", endpoint_var));
                context.insert("endpoint_var", &endpoint_var);
                context.insert("access_key_var_literal", &format!("{:?}", access_key_var));
                context.insert("access_key_var", &access_key_var);
                context.insert("secret_key_var_literal", &format!("{:?}", secret_key_var));
                context.insert("secret_key_var", &secret_key_var);
                context.insert("bucket_literal", &format!("{:?}", bucket));
                render("file/main/s3_infra.rs.tera", &context)
            }
            InfraKind::HttpClient { base_url, headers } => {
                #[derive(serde::Serialize)]
                struct HeaderView {
                    name: String,
                    value_expr: String,
                }
                let headers: Vec<HeaderView> = headers
                    .iter()
                    .map(|(name, value)| HeaderView {
                        name: format!("{:?}", name),
                        value_expr: env_template::to_rust_expr(value),
                    })
                    .collect();

                let mut context = Context::new();
                context.insert("var", &infra_var);
                context.insert("name", &format!("{}Client", infra.infra_name));
                context.insert("base_url_expr", &env_template::to_rust_expr(base_url));
                context.insert("headers", &headers);
                render("file/main/http_client_infra.rs.tera", &context)
            }
        }
    }

    fn gen_use_case(use_case: &UseCaseBinding) -> String {
        let args: Vec<String> = use_case
            .infra_names
            .iter()
            .map(|infra_name| format!("{}_storage", infra_name.to_snake_case()))
            .collect();

        let mut context = Context::new();
        context.insert("field", &use_case.use_case_name.to_snake_case());
        context.insert("name", &use_case.use_case_name);
        context.insert("args", &args);
        render("file/main/use_case.rs.tera", &context)
    }

    fn gen_adapter(adapter: &AdapterBinding) -> String {
        let var = adapter.adapter_name.to_snake_case();
        let mut params: Vec<String> = adapter.use_cases.iter().map(|u| u.to_snake_case()).collect();
        for infra_name in &adapter.auth_infra_names {
            params.push(format!("{}_storage", infra_name.to_snake_case()));
        }
        let address_expr = env_template::to_rust_expr(&adapter.on);

        let mut context = Context::new();
        context.insert("var", &var);
        context.insert("name", &adapter.adapter_name);
        context.insert("params", &params);
        context.insert("address", &address_expr);
        render("file/main/adapter.rs.tera", &context)
    }

    /// Unlike HTTP adapters, tonic's `Server::serve` takes a `SocketAddr`,
    /// not a bindable-address string, so the generated code parses it —
    /// fallibly, via `?`, which is fine since `main` already returns a
    /// boxed-error `Result`.
    fn gen_grpc_adapter(adapter: &GrpcAdapterBinding) -> String {
        let var = adapter.adapter_name.to_snake_case();
        let params: Vec<String> = adapter.use_cases.iter().map(|u| u.to_snake_case()).collect();
        let address_expr = env_template::to_rust_expr(&adapter.on);

        let mut context = Context::new();
        context.insert("var", &var);
        context.insert("name", &adapter.adapter_name);
        context.insert("params", &params);
        context.insert("address", &address_expr);
        render("file/main/grpc_adapter.rs.tera", &context)
    }

    /// `instances` copies of `lb.adapter_name` sharing one `Arc`, each
    /// spawned as its own axum server on an internal-only address, plus one
    /// more task running the round-robin proxy on the publicly-declared
    /// `on` address. Returns the generated code and every task variable
    /// name it introduced, so the caller can await all of them.
    /// Unlike an adapter, a load balancer never constructs anything of its
    /// own — it just spawns the adapter-agnostic proxy task pointed at its
    /// (real, independently-deployed) `backend_addresses`.
    fn gen_load_balancer(lb: &LoadBalancerRuntime) -> (String, Vec<String>) {
        let lb_var = lb.name.to_snake_case();

        // Rust string literals of each backend address.
        let targets: Vec<String> = lb.backend_addresses.iter().map(|a| format!("{:?}", a)).collect();
        let address_expr = env_template::to_rust_expr(&lb.on);
        let proxy_task_var = format!("{}_task", lb_var);

        let mut context = Context::new();
        context.insert("lb_var", &lb_var);
        context.insert("targets", &targets);
        context.insert("address", &address_expr);
        context.insert("proxy_task_var", &proxy_task_var);
        let code = render("file/main/load_balancer.rs.tera", &context);

        (code, vec![proxy_task_var])
    }

    pub fn write(
        &mut self,
        package_name: &str,
        infras: &[InfraBinding],
        use_cases: &[UseCaseBinding],
        adapters: &[AdapterBinding],
        grpc_adapters: &[GrpcAdapterBinding],
        load_balancers: &[LoadBalancerRuntime],
        schedule_jobs: &[ScheduleJobRuntime],
    ) -> Result<(), String> {
        let mut lib_items: Vec<String> = Vec::new();
        for infra in infras {
            lib_items.push(match &infra.kind {
                InfraKind::Psql { .. } => format!("Psql{}Storage", infra.entity_name),
                InfraKind::GrpcClient { .. } => format!("Grpc{}Client", infra.entity_name),
                InfraKind::S3 { .. } => format!("S3{}Storage", infra.entity_name),
                InfraKind::HttpClient { .. } => format!("{}Client", infra.infra_name),
            });
        }
        for use_case in use_cases {
            lib_items.push(use_case.use_case_name.clone());
        }
        for adapter in adapters {
            lib_items.push(adapter.adapter_name.clone());
        }
        for adapter in grpc_adapters {
            lib_items.push(adapter.adapter_name.clone());
            lib_items.push(format!("{}Server", adapter.adapter_name));
        }
        if !load_balancers.is_empty() {
            lib_items.push("RoundRobinProxy".to_string());
            lib_items.push("load_balancer_router".to_string());
        }
        if !schedule_jobs.is_empty() {
            lib_items.push("TrustCron".to_string());
            lib_items.push("run_scheduled_job".to_string());
        }
        let lib_items: Vec<&str> = lib_items.iter().map(String::as_str).collect();
        let crate_ident = package_name.replace('-', "_");

        // Each adapter / gRPC adapter / load balancer is one spawned block;
        // every task it starts is awaited at the end of `main`.
        let mut blocks: Vec<String> = Vec::new();
        let mut task_vars: Vec<String> = Vec::new();
        for adapter in adapters {
            blocks.push(Self::gen_adapter(adapter));
            task_vars.push(format!("{}_task", adapter.adapter_name.to_snake_case()));
        }
        for adapter in grpc_adapters {
            blocks.push(Self::gen_grpc_adapter(adapter));
            task_vars.push(format!("{}_task", adapter.adapter_name.to_snake_case()));
        }
        for lb in load_balancers {
            let (lb_code, lb_task_vars) = Self::gen_load_balancer(lb);
            blocks.push(lb_code);
            task_vars.extend(lb_task_vars);
        }
        for (i, job) in schedule_jobs.iter().enumerate() {
            let var = format!("schedule_job_{}_task", i);
            let mut context = Context::new();
            context.insert("var", &var);
            context.insert("name_literal", &format!("{:?}", job.name));
            context.insert("cron_text", &job.cron_text);
            context.insert("cron", &serde_json::json!({
                "minutes": format!("{:#x}", job.cron.minutes),
                "hours": format!("{:#x}", job.cron.hours),
                "days": format!("{:#x}", job.cron.days),
                "months": format!("{:#x}", job.cron.months),
                "weekdays": format!("{:#x}", job.cron.weekdays),
                "any_day": job.cron.any_day,
                "any_weekday": job.cron.any_weekday,
            }));
            context.insert("tz_literal", &format!("{:?}", job.tz));
            context.insert("use_case", &job.use_case.to_snake_case());
            context.insert("user", &job.user);
            blocks.push(render("file/main/schedule_job.rs.tera", &context));
            task_vars.push(var);
        }

        // Foreign keys reference other tables: added once all exist.
        let mut foreign_keys = String::new();
        for infra in infras {
            if let InfraKind::Psql { foreign_keys: true, .. } = infra.kind {
                let mut context = Context::new();
                context.insert("file", &migration::foreign_keys_file_name(&infra.entity_name));
                context.insert("db_var", &infra.infra_name.to_snake_case());
                foreign_keys.push_str(&render("file/main/migration.rs.tera", &context));
            }
        }

        // `<binary> mcp`: the first MCP adapter over stdio instead of servers.
        let mcp_stdio = adapters.iter().find_map(|a| {
            a.mcp_stdio_user.map(|user| {
                serde_json::json!({
                    "name": a.adapter_name,
                    "params": a.use_cases.iter().map(|u| u.to_snake_case()).collect::<Vec<_>>(),
                    "user": user,
                })
            })
        });

        let mut context = Context::new();
        context.insert("mcp_stdio", &mcp_stdio);
        context.insert("has_schedules", &!schedule_jobs.is_empty());
        context.insert("token_command", &adapters.iter().any(|a| a.requires_jwt));
        context.insert("lib_use", &use_statement(&crate_ident, &lib_items));
        context.insert(
            "infras",
            &(infras.iter().map(Self::gen_infra).collect::<String>() + &foreign_keys),
        );
        context.insert("use_cases", &use_cases.iter().map(Self::gen_use_case).collect::<String>());
        context.insert("blocks", &blocks);
        context.insert("task_vars", &task_vars);
        let code = render("file/main/main.rs.tera", &context);

        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to main.rs: {}", e))
    }
}
