use std::path::PathBuf;

use heck::ToSnakeCase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub fn as_axum_fn(&self) -> &'static str {
        match self {
            HttpMethod::Get => "get",
            HttpMethod::Post => "post",
            HttpMethod::Put => "put",
            HttpMethod::Patch => "patch",
            HttpMethod::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatusMapping {
    pub target: String,
    pub status: u16,
}

/// Extracts `:name` path parameter names from a route path, e.g.
/// `"/users/:id"` -> `["id"]`.
pub fn path_params_of(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|seg| seg.strip_prefix(':').map(|s| s.to_string()))
        .collect()
}

/// Converts a `:name`-style path (this DSL's syntax) into axum 0.8's
/// `{name}`-style path for router registration.
pub fn to_axum_path(path: &str) -> String {
    path.split('/')
        .map(|seg| match seg.strip_prefix(':') {
            Some(name) => format!("{{{}}}", name),
            None => seg.to_string(),
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// A route's `auth { infra: ... target: "..." action: "..." }` block: before
/// the use case runs, the request's JWT (via `auth::UserContext`) is checked
/// against `infra`'s storage for a permission row matching `target`/`action`.
#[derive(Debug, Clone)]
pub struct AuthRequirement {
    pub infra: String,
    pub target: String,
    pub action: String,
}

#[derive(Debug, Clone)]
pub struct HttpRoute {
    pub method: HttpMethod,
    pub path: String,
    pub input: Option<AllowedType>,
    pub output: Option<AllowedType>,
    pub use_case: String,
    pub status: Vec<StatusMapping>,
    pub auth: Option<AuthRequirement>,
}

impl HttpRoute {
    pub fn path_params(&self) -> Vec<String> {
        path_params_of(&self.path)
    }
}

#[derive(Debug, Clone)]
pub struct HttpAdapterService {
    pub routes: Vec<HttpRoute>,
    pub on: Option<String>,
}

/// One `rpc Name { input: X output: Y use_case: Z }` inside a `grpc {
/// services { ... } }` block — the gRPC equivalent of an `HttpRoute`.
#[derive(Debug, Clone)]
pub struct GrpcRpc {
    pub name: String,
    pub input: Option<AllowedType>,
    pub output: Option<AllowedType>,
    pub use_case: String,
}

#[derive(Debug, Clone)]
pub struct GrpcAdapterService {
    pub rpcs: Vec<GrpcRpc>,
    pub on: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AdapterType {
    Http(HttpAdapterService),
    Grpc(GrpcAdapterService),
}

#[derive(Debug, Clone)]
pub struct AdapterService {
    pub name: String,
    pub adapter_type: AdapterType,
}

#[derive(Debug, Clone)]
pub struct PsqlInfraService {
    pub entity: AllowedType,
}

/// One `<storage_method>: <RpcName>` line inside a `grpc_client { methods {
/// ... } }` block — maps a `Storage<T, E>` verb (e.g. `find_one`) to the
/// name of a `rpc` declared on the target `service`'s `grpc {}` adapter.
#[derive(Debug, Clone)]
pub struct GrpcClientMethodMapping {
    pub storage_method: String,
    pub rpc: String,
}

/// `storage { grpc_client { ... } }`: backs a `Storage<T, E>` impl (same
/// trait `psql` storage implements) with a real gRPC call to a `grpc {}`
/// adapter declared elsewhere in the same file — normally in a different
/// `app`, so this is how one generated binary calls into another. `on` is a
/// static target address (no env var, unlike psql's connection string).
#[derive(Debug, Clone)]
pub struct GrpcClientInfraService {
    pub service: String,
    pub on: String,
    pub entity: AllowedType,
    pub methods: Vec<GrpcClientMethodMapping>,
}

/// `storage { s3 { bucket: "..." entity: MediaObject } }`: an object-storage
/// backend (MinIO or any S3-compatible service) for blob data — photos,
/// videos, arbitrary files. Unlike `psql`/`grpc_client`, this does **not**
/// implement the shared `Storage<T, E>` trait: objects aren't rows, so a
/// `use_case` binds to it via `blob:` (see `BlobCall`), not `call:`.
/// `entity:` is metadata only (e.g. `key`, `content_type`) — the object
/// bytes themselves never pass through a Trust-generated struct. Endpoint,
/// access key, and secret key are never written in `.tr` source (same
/// "credentials are always environment-only" precedent `psql`'s connection
/// string already sets) — they're always read from three env vars derived
/// from the infra name at runtime.
#[derive(Debug, Clone)]
pub struct S3InfraService {
    pub bucket: String,
    pub entity: AllowedType,
}

#[derive(Debug, Clone)]
pub enum StorageType {
    Psql(PsqlInfraService),
    GrpcClient(GrpcClientInfraService),
    S3(S3InfraService),
}

#[derive(Debug, Clone)]
pub enum InfraServiceType {
    Storage(StorageType),
}

#[derive(Debug, Clone)]
pub struct InfraService {
    pub name: String,
    pub service_type: InfraServiceType,
}

#[derive(Debug, Clone)]
pub struct ErrorMapping {
    pub source: String,
    pub target: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UseCaseCall {
    pub target: String,
    pub method: String,
    /// This call's own `errors {}` mapping (declared via the `call: {
    /// name: ..., errors { ... } }` block form). Each call in a use case can
    /// map its storage errors differently.
    pub errors: Vec<ErrorMapping>,
}

/// `ai { infra: X prompt: "..." }`: instead of a deterministic CRUD
/// template, the use case's entire `execute()` body is written by an AI
/// model at generation time — see `AiConfig` for the provider/key/model
/// (always passed as CLI flags, never hardcoded). `infra` is optional: some
/// custom logic doesn't touch storage at all.
#[derive(Debug, Clone)]
pub struct AiSpec {
    pub infra: Option<String>,
    pub prompt: String,
}

/// `blob: Target.operation` — binds a use case to one `s3 {}` infra
/// operation. Unlike `UseCaseCall`, there's exactly one per use case (no
/// declaration-order chaining) and no `errors {}` customization — same
/// "fixed `{NotFound, StorageError(String)}` enum" precedent `ai {}` use
/// cases already follow. `operation` is validated at codegen time (one of
/// `presigned_put_url`/`presigned_get_url`/`delete`), not here — mirrors how
/// `grpc_client`'s `methods {}` storage-method names are validated at
/// codegen, not parse time.
#[derive(Debug, Clone)]
pub struct BlobCall {
    pub target: String,
    pub operation: String,
}

/// One `<field>: input.<input_field>` line inside a use case's `filters {}`
/// block — an equality predicate scoping a `list`-bound primary call's
/// results, comparing a storage field against one of the use case's own
/// input fields (e.g. `spot_id: input.spot_id` for "ratings where
/// spot_id = the route's :spot_id"). Only meaningful when the primary
/// `call:` is bound to `list` — validated at codegen, not parse time, same
/// split as `grpc_client`'s `methods {}` storage-verb names.
#[derive(Debug, Clone)]
pub struct FilterMapping {
    pub field: String,
    pub input_field: String,
}

#[derive(Debug, Clone)]
pub struct UseCaseService {
    pub name: String,
    pub input: Option<AllowedType>,
    pub output: Option<AllowedType>,
    /// One or more `call: Target.method` lines, in declaration order. The
    /// *last* call is the primary one — it decides the use case's output
    /// shape and CRUD semantics (`NotFound`, list wrapping, ...). Any
    /// earlier calls run first, as side effects / guards, sharing the same
    /// `input`.
    pub calls: Vec<UseCaseCall>,
    /// Mutually exclusive with `calls`/`blob`: an AI-authored body instead
    /// of the deterministic CRUD template.
    pub ai: Option<AiSpec>,
    /// Mutually exclusive with `calls`/`ai`: binds directly to one `s3 {}`
    /// infra operation instead of the CRUD `Storage<T, E>` template.
    pub blob: Option<BlobCall>,
    /// Zero or more equality predicates narrowing a `list`-bound primary
    /// `call:`'s results — see `FilterMapping`. Empty unless a `filters {}`
    /// block was declared.
    pub filters: Vec<FilterMapping>,
}

#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub name: String,
    pub aliased_type: AllowedType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowedType {
    String,
    Uuid,
    Int,
    Float,
    Decimal,
    Regex(String),
    DefinedType(String),
}

impl AllowedType {
    pub fn to_file_name(&self) -> String {
        match self {
            AllowedType::String => "string".to_string(),
            AllowedType::Uuid => "uuid".to_string(),
            AllowedType::Int => "int".to_string(),
            AllowedType::Float => "float".to_string(),
            AllowedType::Decimal => "decimal".to_string(),
            AllowedType::Regex(_) => "regex".to_string(),
            AllowedType::DefinedType(name) => name.to_snake_case(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub field_type: AllowedType,
}

#[derive(Debug, Clone)]
pub struct StructDeclaration {
    pub name: String,
    pub fields: Vec<StructField>,
    pub export: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TypeDeclaration {
    TypeAlias(TypeAlias),
    Struct(StructDeclaration),
}

impl TypeDeclaration {
    pub fn export_name(&self) -> Option<&str> {
        match self {
            TypeDeclaration::Struct(s) => s.export.as_deref(),
            TypeDeclaration::TypeAlias(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModuleKind {
    Path(PathBuf),
    Git(String),
}

#[derive(Debug, Clone)]
pub enum ImportItem {
    All(String),
    Specific(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub module: ModuleKind,
    pub items: ImportItem,
}

/// `load_balancer Name { targets { "host:port" ... } on: "..." }` inside
/// `app:run`: a round-robin reverse proxy, exposed on `on`, forwarding to
/// `targets` — real network addresses of independently deployed processes
/// (typically the same binary from a *different* named `app`, run once per
/// target address, each on its own machine). The proxy is adapter-agnostic
/// (`templates/file/load_balancer.rs`) — it never constructs or knows
/// anything about what's behind each address.
#[derive(Debug, Clone)]
pub struct LoadBalancerBinding {
    pub name: String,
    pub targets: Vec<String>,
    pub on: String,
}

#[derive(Debug, Clone)]
pub struct AppService {
    pub name: String,
    pub adapters: Vec<String>,
    pub load_balancers: Vec<LoadBalancerBinding>,
}

/// A top-level construct triggered by a plugin-registered keyword. The
/// compiler treats `raw_body` as fully opaque text (everything between the
/// construct's `{` and matching `}`, exclusive) — only the owning plugin,
/// found by matching `plugin_name` against a `LoadedPlugin`'s `keyword`,
/// knows how to interpret it.
#[derive(Debug, Clone)]
pub struct PluginAst {
    pub plugin_name: String,
    pub raw_body: String,
}

#[derive(Debug, Clone)]
pub enum Ast {
    Import(ImportDeclaration),
    Adapter(AdapterService),
    UseCase(UseCaseService),
    Infra(InfraService),
    TypeDecl(TypeDeclaration),
    App(AppService),
    Plugin(PluginAst),
}
