use std::{fs::File, io::Write, path::PathBuf};

use heck::{ToShoutySnakeCase, ToSnakeCase};

use crate::generator::rust_code::{
    env_template, imports::use_statement, migration, render::render,
};
use tera::Context;

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
            InfraKind::Psql { seed } => {
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
        let args = use_case
            .infra_names
            .iter()
            .map(|infra_name| format!("{}_storage.clone()", infra_name.to_snake_case()))
            .collect::<Vec<_>>()
            .join(", ");

        let mut context = Context::new();
        context.insert("field", &use_case.use_case_name.to_snake_case());
        context.insert("name", &use_case.use_case_name);
        context.insert("args", &args);
        render("file/main/use_case.rs.tera", &context)
    }

    fn gen_adapter(adapter: &AdapterBinding) -> String {
        let var = adapter.adapter_name.to_snake_case();
        let mut params: Vec<String> = adapter.use_cases.iter().map(|u| format!("{}.clone()", u.to_snake_case())).collect();
        for infra_name in &adapter.auth_infra_names {
            params.push(format!("{}_storage.clone()", infra_name.to_snake_case()));
        }
        let params = params.join(", ");
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
        let params = adapter.use_cases.iter().map(|u| format!("{}.clone()", u.to_snake_case())).collect::<Vec<_>>().join(", ");
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

        let targets = lb
            .backend_addresses
            .iter()
            .map(|a| format!("{:?}.to_string()", a))
            .collect::<Vec<_>>()
            .join(", ");
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
    ) -> Result<(), String> {
        let mut code = String::new();

        code.push_str("use common::{HttpPresenter, SQLConnector, start_http_server};\n");

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
        let lib_items: Vec<&str> = lib_items.iter().map(String::as_str).collect();
        let crate_ident = package_name.replace('-', "_");
        code.push_str(&use_statement(&crate_ident, &lib_items));
        code.push('\n');

        code.push_str("#[tokio::main]\n");
        code.push_str("async fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
        code.push_str("    dotenvy::dotenv().ok();\n\n");

        for infra in infras {
            code.push_str(&Self::gen_infra(infra));
        }
        code.push('\n');

        for use_case in use_cases {
            code.push_str(&Self::gen_use_case(use_case));
        }
        code.push('\n');

        for adapter in adapters {
            code.push_str(&Self::gen_adapter(adapter));
            code.push('\n');
        }

        for adapter in grpc_adapters {
            code.push_str(&Self::gen_grpc_adapter(adapter));
            code.push('\n');
        }

        let mut all_task_vars: Vec<String> = adapters
            .iter()
            .map(|a| format!("{}_task", a.adapter_name.to_snake_case()))
            .chain(grpc_adapters.iter().map(|a| format!("{}_task", a.adapter_name.to_snake_case())))
            .collect();
        for lb in load_balancers {
            let (lb_code, task_vars) = Self::gen_load_balancer(lb);
            code.push_str(&lb_code);
            code.push('\n');
            all_task_vars.extend(task_vars);
        }

        for var in &all_task_vars {
            code.push_str(&format!("    {var}.await??;\n", var = var));
        }
        code.push('\n');
        code.push_str("    Ok(())\n");
        code.push_str("}\n");

        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write to main.rs: {}", e))
    }
}
