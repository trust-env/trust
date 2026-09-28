pub mod adapter;
pub mod app;
pub mod domains;
pub mod import;
pub mod infra;
pub mod plugin;
pub mod rust_code;
pub mod types;
pub mod use_case;
mod workspace;

pub use infra::*;

use std::{
    collections::HashMap,
    fs::{File, create_dir, create_dir_all, remove_dir_all},
    path::PathBuf,
};

use heck::ToSnakeCase;

use crate::{
    ai::AiConfig,
    generator::rust_code::{
        cargo_toml::{CargoToml, Dependency, WorkspaceCargoToml},
        file::{load_balancer_file::LoadBalancerFile, mod_file::ModFile},
        types::RustType,
    },
    parser::{AllowedType, Ast, ErrorMapping, InfraServiceType::Storage, StorageType::Psql},
    plugin::LoadedPlugin,
};

pub struct CodeGen {
    asts: Vec<Ast>,
    position: usize,
}

/// The resolved (post-guessing) shape of a use case, kept around purely for
/// `--dev` documentation: field names/types are display strings, not tied to
/// the actual generated Rust types.
pub struct UseCaseIo {
    /// The actual Rust type name a route handler must reference for the
    /// input: `{UseCase}Input` for CRUD use cases, or the declared type's
    /// own name (e.g. `CreateTaskRequest`) for `ai {}` use cases, which use
    /// their declared type directly rather than a synthetic wrapper.
    pub input_type_name: String,
    pub input_fields: Vec<(String, String)>,
    /// Same idea as `input_type_name`, for the output.
    pub output_type_name: String,
    pub output_entity: String,
    pub output_fields: Vec<(String, String)>,
    pub output_is_list: bool,
    /// CRUD use cases wrap their entity in a single-field struct — e.g.
    /// `CreateUseCaseOutput { user: User }` — so `output_type_name`'s real
    /// fields are just this one (named after the entity, holding it
    /// directly), *not* `output_fields` flattened. `ai {}` use cases have
    /// no wrapper: their output type's fields are `output_fields` exactly,
    /// so this is `None`.
    pub output_wrapper_field: Option<String>,
    /// The use case reads the caller's id (`owner:`), so every route
    /// exposing it must declare `auth` — without it, handlers pass user 0.
    pub requires_user: bool,
    /// The input is the built-in `RawRequest` (raw body + headers) rather
    /// than a JSON/query-decoded struct.
    pub raw_input: bool,
}

pub struct CodeGenContext {
    types: Vec<RustType>,
    infra_entities: HashMap<String, AllowedType>,
    /// Static target addresses for `grpc_client` infras (the `on:` field) —
    /// presence in this map is how `register_infra_binding` tells a
    /// grpc-client infra apart from a psql one, since both register an
    /// entity in `infra_entities` the same way.
    infra_target_addresses: HashMap<String, String>,
    /// `s3 {}` infra entities, kept deliberately separate from
    /// `infra_entities` (rather than reusing it like `psql`/`grpc_client`
    /// do) so `is_storage_entity` — which drives `sqlx::FromRow` + the
    /// `sqlx` dependency in `gen_domain` — never fires for a blob-storage
    /// entity. Blob entities are metadata only; they're never queried via
    /// `sqlx::query_as`.
    blob_infra_entities: HashMap<String, AllowedType>,
    /// The literal `bucket:` string per `s3 {}` infra (never env-driven,
    /// unlike endpoint/credentials — see `S3InfraService`'s doc comment).
    blob_infra_buckets: HashMap<String, String>,
    use_case_signatures: HashMap<String, (AllowedType, AllowedType)>,
    known_use_cases: Vec<String>,
    /// Use cases declared with `ai {}` and no `infra:` — legitimately have
    /// zero storage bindings, unlike a CRUD use case (which always needs at
    /// least one `call`).
    ai_use_cases_without_infra: Vec<String>,
    use_case_infra: HashMap<String, Vec<String>>,
    use_case_errors: HashMap<String, Vec<ErrorMapping>>,
    /// Infra names whose `psql {}` block declares a `seed:` file.
    psql_seeded_infras: Vec<String>,
    use_case_io: HashMap<String, UseCaseIo>,
    dependencies: HashMap<String, Dependency>,
    build_dependencies: HashMap<String, Dependency>,
    domain_modules: Vec<String>,
    use_case_modules: Vec<String>,
    adapter_http_modules: Vec<String>,
    adapter_grpc_modules: Vec<String>,
    infra_entity_modules: Vec<String>,
    /// Module names (bare, no `.rs`) written under
    /// `src/infrastructures/storage/grpc_client/` — mirrors
    /// `infra_entity_modules`, kept separate so `storage.rs` only declares
    /// `mod grpc_client;` when at least one `grpc_client` infra was
    /// actually generated.
    grpc_client_entity_modules: Vec<String>,
    /// Module names (bare, no `.rs`) written under
    /// `src/infrastructures/storage/s3/` — same opt-in pattern as
    /// `grpc_client_entity_modules`.
    s3_entity_modules: Vec<String>,
    /// `http_client {}` infras, by infra name.
    http_client_infras: HashMap<String, crate::parser::HttpClientInfraService>,
    /// Module names written under `src/infrastructures/http_client/`.
    http_client_modules: Vec<String>,
    /// Module names (bare, no `.rs`) written under `src/plugins/` by
    /// `generate_plugin` — derived from each plugin codegen call's own
    /// `output_file`, which need not match the owning plugin's `name`.
    plugin_modules: Vec<String>,
    /// `.proto` file paths written by every gRPC adapter — collected here
    /// so a single aggregate `build.rs` can compile all of them.
    grpc_proto_paths: Vec<PathBuf>,
}

impl CodeGenContext {
    pub fn new() -> Self {
        CodeGenContext {
            types: Vec::new(),
            infra_entities: HashMap::new(),
            infra_target_addresses: HashMap::new(),
            blob_infra_entities: HashMap::new(),
            blob_infra_buckets: HashMap::new(),
            use_case_signatures: HashMap::new(),
            known_use_cases: Vec::new(),
            ai_use_cases_without_infra: Vec::new(),
            use_case_infra: HashMap::new(),
            use_case_errors: HashMap::new(),
            psql_seeded_infras: Vec::new(),
            use_case_io: HashMap::new(),
            dependencies: HashMap::new(),
            build_dependencies: HashMap::new(),
            domain_modules: Vec::new(),
            use_case_modules: Vec::new(),
            adapter_http_modules: Vec::new(),
            adapter_grpc_modules: Vec::new(),
            infra_entity_modules: Vec::new(),
            grpc_client_entity_modules: Vec::new(),
            s3_entity_modules: Vec::new(),
            http_client_infras: HashMap::new(),
            http_client_modules: Vec::new(),
            plugin_modules: Vec::new(),
            grpc_proto_paths: Vec::new(),
        }
    }

    /// Records a `src/plugins/<module>.rs` file written by `generate_plugin`,
    /// deduped, so `run_with_plugins` can declare it in `plugins.rs`.
    pub fn register_plugin_module(&mut self, module: String) {
        if !self.plugin_modules.contains(&module) {
            self.plugin_modules.push(module);
        }
    }

    pub fn add_type(&mut self, rust_type: RustType) {
        self.types.push(rust_type);
    }

    pub fn find_type(&self, name: &str) -> Option<&RustType> {
        self.types.iter().find(|t| t.name == name)
    }

    pub fn register_http_client_infra(
        &mut self,
        infra_name: String,
        cfg: crate::parser::HttpClientInfraService,
    ) {
        self.http_client_infras.insert(infra_name, cfg);
    }

    pub fn find_http_client_infra(
        &self,
        infra_name: &str,
    ) -> Option<&crate::parser::HttpClientInfraService> {
        self.http_client_infras.get(infra_name)
    }

    pub fn register_http_client_module(&mut self, name: String) {
        if !self.http_client_modules.contains(&name) {
            self.http_client_modules.push(name);
        }
    }

    pub fn register_psql_seed(&mut self, infra_name: String) {
        if !self.psql_seeded_infras.contains(&infra_name) {
            self.psql_seeded_infras.push(infra_name);
        }
    }

    pub fn has_psql_seed(&self, infra_name: &str) -> bool {
        self.psql_seeded_infras.iter().any(|n| n == infra_name)
    }

    pub fn register_infra_entity(&mut self, infra_name: String, entity: AllowedType) {
        self.infra_entities.insert(infra_name, entity);
    }

    pub fn find_infra_entity(&self, infra_name: &str) -> Option<&AllowedType> {
        self.infra_entities.get(infra_name)
    }

    pub fn register_infra_target_address(&mut self, infra_name: String, address: String) {
        self.infra_target_addresses.insert(infra_name, address);
    }

    pub fn find_infra_target_address(&self, infra_name: &str) -> Option<&str> {
        self.infra_target_addresses.get(infra_name).map(String::as_str)
    }

    pub fn register_blob_infra(&mut self, infra_name: String, bucket: String, entity: AllowedType) {
        self.blob_infra_entities.insert(infra_name.clone(), entity);
        self.blob_infra_buckets.insert(infra_name, bucket);
    }

    pub fn find_blob_infra_entity(&self, infra_name: &str) -> Option<&AllowedType> {
        self.blob_infra_entities.get(infra_name)
    }

    pub fn find_blob_infra_bucket(&self, infra_name: &str) -> Option<&str> {
        self.blob_infra_buckets.get(infra_name).map(String::as_str)
    }

    pub fn is_blob_infra(&self, infra_name: &str) -> bool {
        self.blob_infra_entities.contains_key(infra_name)
    }

    pub fn register_use_case_signature(
        &mut self,
        use_case_name: String,
        input: AllowedType,
        output: AllowedType,
    ) {
        self.use_case_signatures.insert(use_case_name, (input, output));
    }

    pub fn find_use_case_signature(&self, use_case_name: &str) -> Option<&(AllowedType, AllowedType)> {
        self.use_case_signatures.get(use_case_name)
    }

    pub fn register_known_use_case(&mut self, use_case_name: String) {
        if !self.known_use_cases.contains(&use_case_name) {
            self.known_use_cases.push(use_case_name);
        }
    }

    pub fn is_known_use_case(&self, use_case_name: &str) -> bool {
        self.known_use_cases.iter().any(|n| n == use_case_name)
    }

    pub fn register_ai_use_case_without_infra(&mut self, use_case_name: String) {
        if !self.ai_use_cases_without_infra.contains(&use_case_name) {
            self.ai_use_cases_without_infra.push(use_case_name);
        }
    }

    pub fn is_ai_use_case_without_infra(&self, use_case_name: &str) -> bool {
        self.ai_use_cases_without_infra.iter().any(|n| n == use_case_name)
    }

    /// Records an infra target used by a use case's `call:` lines, in
    /// declaration order (deduped) — a use case may call more than one
    /// infra/service.
    pub fn register_use_case_infra(&mut self, use_case_name: String, infra_name: String) {
        let entry = self.use_case_infra.entry(use_case_name).or_default();
        if !entry.contains(&infra_name) {
            entry.push(infra_name);
        }
    }

    pub fn find_use_case_infra(&self, use_case_name: &str) -> &[String] {
        self.use_case_infra
            .get(use_case_name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn register_use_case_errors(&mut self, use_case_name: String, errors: Vec<ErrorMapping>) {
        self.use_case_errors.insert(use_case_name, errors);
    }

    pub fn find_use_case_errors(&self, use_case_name: &str) -> &[ErrorMapping] {
        self.use_case_errors
            .get(use_case_name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn register_use_case_io(&mut self, use_case_name: String, io: UseCaseIo) {
        self.use_case_io.insert(use_case_name, io);
    }

    pub fn find_use_case_io(&self, use_case_name: &str) -> Option<&UseCaseIo> {
        self.use_case_io.get(use_case_name)
    }

    pub fn require_git_dependency(&mut self, name: &str, git_url: &str) {
        self.dependencies
            .entry(name.to_string())
            .or_insert_with(|| Dependency::Git(git_url.to_string()));
    }

    pub fn require_version_dependency(&mut self, name: &str, version: &str) {
        self.dependencies
            .entry(name.to_string())
            .or_insert_with(|| Dependency::Version(version.to_string()));
    }

    pub fn require_version_dependency_with_features(
        &mut self,
        name: &str,
        version: &str,
        features: Vec<&str>,
    ) {
        match self
            .dependencies
            .entry(name.to_string())
            .or_insert_with(|| Dependency::VersionWithFeatures(version.to_string(), Vec::new()))
        {
            Dependency::VersionWithFeatures(_, existing) => {
                for feature in features {
                    if !existing.iter().any(|f| f == feature) {
                        existing.push(feature.to_string());
                    }
                }
            }
            // Already required without features (e.g. `reqwest` by a load
            // balancer) — keep its version, add the features.
            dependency @ Dependency::Version(_) => {
                if let Dependency::Version(existing_version) = dependency {
                    *dependency = Dependency::VersionWithFeatures(
                        existing_version.clone(),
                        features.into_iter().map(str::to_string).collect(),
                    );
                }
            }
            _ => {}
        }
    }

    pub fn require_build_dependency(&mut self, name: &str, version: &str) {
        self.build_dependencies
            .entry(name.to_string())
            .or_insert_with(|| Dependency::Version(version.to_string()));
    }

    pub fn is_storage_entity(&self, name: &str) -> bool {
        self.infra_entities
            .values()
            .any(|entity| matches!(entity, AllowedType::DefinedType(n) if n == name))
    }

    pub fn register_domain_module(&mut self, name: String) {
        if !self.domain_modules.contains(&name) {
            self.domain_modules.push(name);
        }
    }

    pub fn register_use_case_module(&mut self, name: String) {
        if !self.use_case_modules.contains(&name) {
            self.use_case_modules.push(name);
        }
    }

    pub fn register_adapter_http_module(&mut self, name: String) {
        if !self.adapter_http_modules.contains(&name) {
            self.adapter_http_modules.push(name);
        }
    }

    pub fn register_adapter_grpc_module(&mut self, name: String) {
        if !self.adapter_grpc_modules.contains(&name) {
            self.adapter_grpc_modules.push(name);
        }
    }

    pub fn register_infra_entity_module(&mut self, name: String) {
        if !self.infra_entity_modules.contains(&name) {
            self.infra_entity_modules.push(name);
        }
    }

    pub fn register_grpc_client_entity_module(&mut self, name: String) {
        if !self.grpc_client_entity_modules.contains(&name) {
            self.grpc_client_entity_modules.push(name);
        }
    }

    pub fn register_s3_entity_module(&mut self, name: String) {
        if !self.s3_entity_modules.contains(&name) {
            self.s3_entity_modules.push(name);
        }
    }

    pub fn register_grpc_proto_path(&mut self, path: PathBuf) {
        if !self.grpc_proto_paths.contains(&path) {
            self.grpc_proto_paths.push(path);
        }
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn find_infra_entity_returns_none_before_registration() {
        let ctx = CodeGenContext::new();
        assert!(ctx.find_infra_entity("Users").is_none());
    }

    #[test]
    fn register_infra_entity_tracks_two_distinct_entities_independently() {
        // Seeds the duplicate-folder regression (two `infra {}` blocks for
        // different entities must be tracked separately, not clobber each
        // other) — exercised end-to-end in Phase 2/3.
        let mut ctx = CodeGenContext::new();
        ctx.register_infra_entity("Users".to_string(), AllowedType::DefinedType("User".to_string()));
        ctx.register_infra_entity("Tasks".to_string(), AllowedType::DefinedType("Task".to_string()));

        assert_eq!(
            ctx.find_infra_entity("Users"),
            Some(&AllowedType::DefinedType("User".to_string()))
        );
        assert_eq!(
            ctx.find_infra_entity("Tasks"),
            Some(&AllowedType::DefinedType("Task".to_string()))
        );
    }

    #[test]
    fn re_registering_the_same_infra_name_overwrites_its_entity() {
        let mut ctx = CodeGenContext::new();
        ctx.register_infra_entity("Users".to_string(), AllowedType::DefinedType("User".to_string()));
        ctx.register_infra_entity("Users".to_string(), AllowedType::DefinedType("Admin".to_string()));

        assert_eq!(
            ctx.find_infra_entity("Users"),
            Some(&AllowedType::DefinedType("Admin".to_string()))
        );
    }

    #[test]
    fn is_storage_entity_checks_registered_infra_entities_by_defined_type_name() {
        let mut ctx = CodeGenContext::new();
        ctx.register_infra_entity("Users".to_string(), AllowedType::DefinedType("User".to_string()));

        assert!(ctx.is_storage_entity("User"));
        assert!(!ctx.is_storage_entity("Task"));
        assert!(!ctx.is_storage_entity("Users"));
    }

    #[test]
    fn register_known_use_case_dedups() {
        let mut ctx = CodeGenContext::new();
        ctx.register_known_use_case("CreateUser".to_string());
        ctx.register_known_use_case("CreateUser".to_string());

        assert!(ctx.is_known_use_case("CreateUser"));
        assert!(!ctx.is_known_use_case("DeleteUser"));
    }

    #[test]
    fn register_use_case_infra_dedups_and_preserves_order() {
        let mut ctx = CodeGenContext::new();
        ctx.register_use_case_infra("CreateUser".to_string(), "Users".to_string());
        ctx.register_use_case_infra("CreateUser".to_string(), "Audit".to_string());
        ctx.register_use_case_infra("CreateUser".to_string(), "Users".to_string());

        assert_eq!(ctx.find_use_case_infra("CreateUser"), &["Users".to_string(), "Audit".to_string()]);
    }

    #[test]
    fn find_use_case_infra_is_empty_for_unknown_use_case() {
        let ctx = CodeGenContext::new();
        assert!(ctx.find_use_case_infra("Unknown").is_empty());
    }
}

impl CodeGen {
    pub fn new(asts: Vec<Ast>) -> Self {
        CodeGen { asts, position: 0 }
    }

    fn next(&mut self) -> Option<&Ast> {
        let ast = self.asts.get(self.position);

        self.position += 1;
        ast
    }

    fn check_output_dir(&self, path: &PathBuf) -> Result<(), String> {
        if !path.exists() {
            std::fs::create_dir_all(path)
                .map_err(|e| format!("Failed to create output directory: {}", e))?;
        }
        Ok(())
    }

    pub fn run(&mut self, path: PathBuf, dev_mode: bool, ai_config: &AiConfig) -> Result<(), String> {
        self.run_with_plugins(path, dev_mode, ai_config, &[])
    }

    pub fn run_with_plugins(
        &mut self,
        path: PathBuf,
        dev_mode: bool,
        ai_config: &AiConfig,
        loaded_plugins: &[LoadedPlugin],
    ) -> Result<(), String> {
        self.check_output_dir(&path)?;
        let asts = std::mem::take(&mut self.asts); // moves Vec out, leaves empty Vec behind
        remove_dir_all(path.clone())
            .map_err(|e| format!("Error when removing the folder: {}", e))?;
        create_dir_all(&path)
            .map_err(|e| format!("Error when creating the output folder: {}", e))?;

        let partitioned = workspace::partition_by_app(&asts)?;

        if partitioned.is_empty() {
            // No named `app` to partition by at all (e.g. a fixture that
            // only exercises a single construct's codegen in isolation) —
            // there's nothing to split into per-app crates, so fall back to
            // generating everything into one flat crate at `path`, exactly
            // like before named multi-app support existed.
            self.generate_crate(path, asts.clone(), &asts, dev_mode, ai_config, loaded_plugins)?;
            self.asts = asts;
            return Ok(());
        }

        let mut members = Vec::new();
        for (app, app_asts) in &partitioned {
            let member_name = app.name.to_snake_case();
            let member_dir = path.join(&member_name);
            self.generate_crate(member_dir, app_asts.clone(), &asts, dev_mode, ai_config, loaded_plugins)?;
            members.push(member_name);
        }

        WorkspaceCargoToml::new(path.join("Cargo.toml"))?.write(&members)?;

        self.asts = asts;
        Ok(())
    }

    /// Registers every type/infra-entity/use_case-signature/use_case-IO
    /// fact resolvable from `asts` into a fresh `CodeGenContext`, without
    /// writing any file. This is both the pre-pass `generate_crate` runs
    /// over its own (per-app-filtered) `asts` before generating files, and
    /// — built instead over the *full*, unfiltered ast list — the read-only
    /// lookup a `grpc_client` infra uses to resolve a target rpc's use_case
    /// declared in a *different* app's partition (see
    /// `infra/storage/grpc_client.rs`).
    fn build_lookup_context(&self, asts: &[Ast]) -> Result<CodeGenContext, String> {
        let mut context = CodeGenContext::new();

        for ast in asts {
            match ast {
                Ast::Infra(infra) => match &infra.service_type {
                    Storage(Psql(psql)) => {
                        context.register_infra_entity(infra.name.clone(), psql.entity.clone());
                        if psql.seed.is_some() {
                            context.register_psql_seed(infra.name.clone());
                        }
                    }
                    Storage(crate::parser::StorageType::GrpcClient(cfg)) => {
                        context.register_infra_entity(infra.name.clone(), cfg.entity.clone());
                        context.register_infra_target_address(infra.name.clone(), cfg.on.clone());
                    }
                    Storage(crate::parser::StorageType::S3(cfg)) => {
                        context.register_blob_infra(
                            infra.name.clone(),
                            cfg.bucket.clone(),
                            cfg.entity.clone(),
                        );
                    }
                    crate::parser::InfraServiceType::HttpClient(cfg) => {
                        context.register_http_client_infra(infra.name.clone(), cfg.clone());
                    }
                },
                Ast::UseCase(use_case) => {
                    context.register_known_use_case(use_case.name.clone());
                    if let (Some(input), Some(output)) = (&use_case.input, &use_case.output) {
                        context.register_use_case_signature(
                            use_case.name.clone(),
                            input.clone(),
                            output.clone(),
                        );
                    }
                    let mut all_errors = Vec::new();
                    for call in &use_case.calls {
                        context.register_use_case_infra(use_case.name.clone(), call.target.clone());
                        all_errors.extend(call.errors.iter().cloned());
                    }
                    if !all_errors.is_empty() {
                        context.register_use_case_errors(use_case.name.clone(), all_errors);
                    }
                    if let Some(ai) = &use_case.ai {
                        match &ai.infra {
                            Some(infra_name) => {
                                context.register_use_case_infra(use_case.name.clone(), infra_name.clone());
                            }
                            None => {
                                context.register_ai_use_case_without_infra(use_case.name.clone());
                            }
                        }
                    }
                    if let Some(blob) = &use_case.blob {
                        context.register_use_case_infra(use_case.name.clone(), blob.target.clone());
                    }
                    if let Some(code) = &use_case.code {
                        for infra_name in &code.infras {
                            context
                                .register_use_case_infra(use_case.name.clone(), infra_name.clone());
                        }
                        if code.infras.is_empty() {
                            // Same "constructed with no arguments" wiring
                            // `ai {}` use cases without an infra get.
                            context.register_ai_use_case_without_infra(use_case.name.clone());
                        }
                        // Only the targets matter to routes' `status {}`
                        // validation and status functions — a `code {}`
                        // variant has no storage error source.
                        let errors = code
                            .errors
                            .iter()
                            .map(|variant| ErrorMapping {
                                source: String::new(),
                                target: variant.clone(),
                                message: None,
                            })
                            .collect();
                        context.register_use_case_errors(use_case.name.clone(), errors);
                    }
                }
                _ => {}
            }
        }

        // Types and use_case input/output shapes must be resolvable regardless
        // of where an adapter referencing them sits in file order (adapters
        // are otherwise processed in the same pass, below), so resolve both
        // ahead of time. `gen_domain`/`generate_use_case`/`generate_import`
        // redo this same resolution during the main pass to actually write
        // files; the duplicate work is cheap and both are idempotent.
        for ast in asts {
            if let Ast::Import(import) = ast {
                for resolved in self.resolve_import_types(&mut context, import)? {
                    context.add_type(resolved);
                }
            }
        }
        for ast in asts {
            if let Ast::TypeDecl(t) = ast {
                let resolved = self.resolve_type_declaration(&mut context, t)?;
                context.add_type(resolved);
            }
        }
        for ast in asts {
            if let Ast::UseCase(use_case) = ast {
                self.register_use_case_io(&mut context, use_case)?;
            }
        }

        Ok(context)
    }

    fn generate_crate(
        &mut self,
        path: PathBuf,
        asts: Vec<Ast>,
        full_asts: &[Ast],
        dev_mode: bool,
        ai_config: &AiConfig,
        loaded_plugins: &[LoadedPlugin],
    ) -> Result<(), String> {
        let src_path = path.join("src");
        let adapter_path = src_path.join("adapters");
        let infra_path = src_path.join("infrastructures");
        let domain_path = src_path.join("domains");
        let use_case_path = src_path.join("applications");
        let proto_path = path.join("proto");
        let plugins_path = src_path.join("plugins");

        create_dir_all(&src_path)
            .map_err(|e| format!("Error when creating the folder src: {}", e))?;
        create_dir_all(&adapter_path)
            .map_err(|e| format!("Error when creating the folder adapter: {}", e))?;
        create_dir_all(&infra_path)
            .map_err(|e| format!("Error when creating the folder infra: {}", e))?;
        create_dir_all(&domain_path)
            .map_err(|e| format!("Error when creating the folder domain: {}", e))?;
        create_dir_all(&use_case_path)
            .map_err(|e| format!("Error when creating the folder use_case: {}", e))?;

        let mut context = self.build_lookup_context(&asts)?;

        for ast in &asts {
            match ast {
                Ast::Import(import) => {
                    self.generate_import(&mut context, import.clone(), domain_path.clone())
                }
                Ast::Infra(infra) => self.generate_infra(
                    &mut context,
                    infra.clone(),
                    infra_path.clone(),
                    full_asts,
                    &proto_path,
                ),
                Ast::Adapter(adapter) => self.generate_adapter(
                    &mut context,
                    adapter.clone(),
                    adapter_path.clone(),
                    dev_mode,
                    &proto_path,
                ),
                Ast::UseCase(use_case) => self.generate_use_case(
                    &mut context,
                    use_case.clone(),
                    use_case_path.clone(),
                    ai_config,
                ),
                Ast::TypeDecl(t) => self.gen_domain(&mut context, t, domain_path.clone()),
                Ast::App(_) => Ok(()),
                Ast::Plugin(plugin_ast) => self.generate_plugin(
                    &mut context,
                    plugin_ast,
                    loaded_plugins,
                    &plugins_path,
                    dev_mode,
                ),
            }?
        }

        let storage_dir = infra_path.join("storage");
        let psql_dir = storage_dir.join("psql");
        create_dir_all(&psql_dir)
            .map_err(|e| format!("Error when creating the folder psql: {}", e))?;
        ModFile::new(storage_dir.join("psql.rs"))?
            .with_mods(context.infra_entity_modules.clone())
            .write()?;

        let mut storage_mods = vec!["psql".to_string()];
        if !context.grpc_client_entity_modules.is_empty() {
            let grpc_client_dir = storage_dir.join("grpc_client");
            create_dir_all(&grpc_client_dir)
                .map_err(|e| format!("Error when creating the folder grpc_client: {}", e))?;
            ModFile::new(storage_dir.join("grpc_client.rs"))?
                .with_mods(context.grpc_client_entity_modules.clone())
                .write()?;
            storage_mods.push("grpc_client".to_string());
        }
        if !context.s3_entity_modules.is_empty() {
            let s3_dir = storage_dir.join("s3");
            create_dir_all(&s3_dir).map_err(|e| format!("Error when creating the folder s3: {}", e))?;
            ModFile::new(storage_dir.join("s3.rs"))?
                .with_mods(context.s3_entity_modules.clone())
                .write()?;
            storage_mods.push("s3".to_string());
        }
        ModFile::new(infra_path.join("storage.rs"))?
            .with_mods(storage_mods)
            .write()?;
        let mut infra_mods = vec!["storage".to_string()];
        if !context.http_client_modules.is_empty() {
            let http_client_dir = infra_path.join("http_client");
            create_dir_all(&http_client_dir)
                .map_err(|e| format!("Error when creating the folder http_client: {}", e))?;
            ModFile::new(infra_path.join("http_client.rs"))?
                .with_mods(context.http_client_modules.clone())
                .write()?;
            infra_mods.push("http_client".to_string());
        }
        ModFile::new(src_path.join("infrastructures.rs"))?
            .with_mods(infra_mods)
            .write()?;

        // Only wire up `adapters/http` and `adapters/grpc` (dir + mod file
        // + declaration in `adapters.rs`) for the transports this crate
        // actually uses — mirrors the same "only declare it if something
        // was actually registered" pattern used for `storage/grpc_client`
        // and `plugins` below, so a project with no gRPC (or no HTTP)
        // adapters doesn't get an empty, unused module for it.
        let needs_load_balancer = asts
            .iter()
            .any(|ast| matches!(ast, Ast::App(app) if !app.load_balancers.is_empty()));

        let mut adapter_mods = Vec::new();

        if !context.adapter_http_modules.is_empty() || needs_load_balancer {
            let http_dir = adapter_path.join("http");
            create_dir_all(&http_dir)
                .map_err(|e| format!("Error when creating the folder http: {}", e))?;

            if needs_load_balancer {
                LoadBalancerFile::new(http_dir.clone())?.write()?;
                context.register_adapter_http_module("load_balancer".to_string());
            }

            ModFile::new(adapter_path.join("http.rs"))?
                .with_mods(context.adapter_http_modules.clone())
                .write()?;
            adapter_mods.push("http".to_string());
        }

        if !context.adapter_grpc_modules.is_empty() {
            let grpc_dir = adapter_path.join("grpc");
            create_dir_all(&grpc_dir)
                .map_err(|e| format!("Error when creating the folder grpc: {}", e))?;
            ModFile::new(adapter_path.join("grpc.rs"))?
                .with_mods(context.adapter_grpc_modules.clone())
                .write()?;
            adapter_mods.push("grpc".to_string());
        }

        ModFile::new(src_path.join("adapters.rs"))?
            .with_mods(adapter_mods)
            .write()?;

        if !context.grpc_proto_paths.is_empty() {
            let mut build_rs = String::new();
            build_rs.push_str("fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
            build_rs.push_str("    let protoc_path = protoc_bin_vendored::protoc_bin_path()?;\n");
            build_rs.push_str("    unsafe {\n        std::env::set_var(\"PROTOC\", protoc_path);\n    }\n");
            for proto_path in &context.grpc_proto_paths {
                let relative = proto_path
                    .strip_prefix(&path)
                    .unwrap_or(proto_path)
                    .to_string_lossy()
                    .replace('\\', "/");
                build_rs.push_str(&format!("    tonic_build::compile_protos({:?})?;\n", relative));
            }
            build_rs.push_str("    Ok(())\n}\n");
            std::fs::write(path.join("build.rs"), build_rs)
                .map_err(|e| format!("Failed to write build.rs: {}", e))?;
        }

        ModFile::new(src_path.join("domains.rs"))?
            .with_mods(context.domain_modules.clone())
            .write()?;
        ModFile::new(src_path.join("applications.rs"))?
            .with_mods(context.use_case_modules.clone())
            .write()?;

        let mut lib_mods = vec![
            "domains".to_string(),
            "infrastructures".to_string(),
            "applications".to_string(),
            "adapters".to_string(),
        ];
        if !context.plugin_modules.is_empty() {
            ModFile::new(src_path.join("plugins.rs"))?
                .with_mods(context.plugin_modules.clone())
                .write()?;
            lib_mods.push("plugins".to_string());
        }

        ModFile::new(src_path.join("lib.rs"))?.with_mods(lib_mods).write()?;

        let package_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("generated_project")
            .to_string();

        for ast in &asts {
            if let Ast::App(app) = ast {
                self.generate_app(&mut context, app, &asts, &src_path, &package_name, dev_mode, &path)?;
            }
        }

        CargoToml::new(path.join("Cargo.toml"), package_name)?
            .write(&context.dependencies, &context.build_dependencies)?;

        Ok(())
    }
}
