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

/// A route's `auth` requirement. Either form makes the handler extract the
/// caller's JWT (via `auth::UserContext`, 401 if missing/invalid) and pass
/// its `user_id` to the use case's `execute`.
#[derive(Debug, Clone)]
pub enum AuthRequirement {
    /// `auth: required` — any valid JWT is enough, no permission lookup.
    Authenticated,
    /// `auth { infra: ... target: "..." action: "..." }` — additionally
    /// checks `infra`'s storage for a permission row matching
    /// `target`/`action`.
    Permission(PermissionRequirement),
}

impl AuthRequirement {
    /// The permission-row infra this requirement queries, if any.
    pub fn permission_infra(&self) -> Option<&str> {
        match self {
            AuthRequirement::Authenticated => None,
            AuthRequirement::Permission(p) => Some(&p.infra),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PermissionRequirement {
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
    /// `cors: "${CORS_ORIGINS}"` — comma-separated browser origins allowed
    /// to call this adapter (`*` for any). Supports `${VAR}` interpolation,
    /// like `on:`.
    pub cors: Option<String>,
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

#[derive(Debug, Clone, Default)]
pub struct PsqlInfraService {
    pub entity: AllowedType,
    /// `seed: "./seed/x.sql"` — SQL run at every startup, right after the
    /// entity's table is created. Must be idempotent (e.g. `ON CONFLICT
    /// DO NOTHING`), since it re-runs on every boot.
    pub seed: Option<String>,
    /// `unique: [a, b]` (repeatable) — unique indexes. The first one is the
    /// key `upsert_one` conflicts on.
    pub unique: Vec<Vec<String>>,
    /// `index: [a, b]` (repeatable) — extra indexes.
    pub indexes: Vec<Vec<String>>,
    /// `references { field: OtherInfra [cascade|set_null|restrict] }` —
    /// foreign keys to another psql infra's `id`.
    pub references: Vec<ForeignKey>,
    /// `renamed { old: new }` — columns renamed since the last deploy.
    pub renamed: Vec<(String, String)>,
    /// `dropped: [a, b]` — columns removed since the last deploy.
    pub dropped: Vec<String>,
}

/// One `references {}` entry of a psql infra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub field: String,
    pub infra: String,
    /// `cascade`, `set_null` or `restrict`; `None` picks `set_null` for a
    /// nullable column and `restrict` otherwise.
    pub on_delete: Option<String>,
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
    /// `http_client { base_url: "..." headers { "Name": "value" } }` — a
    /// configured outbound HTTP client (JSON in/out), usable from `code {}`
    /// use cases only.
    HttpClient(HttpClientInfraService),
}

/// `base_url` and header values support `${VAR}` interpolation (resolved
/// at runtime), so secrets like API keys stay out of `.tr` source.
#[derive(Debug, Clone)]
pub struct HttpClientInfraService {
    pub base_url: String,
    pub headers: Vec<(String, String)>,
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

/// One line of a use case's `filters {}` block: `<field> <op> <source>`,
/// a predicate scoping a `list`-bound primary call's results. `field: x`
/// is the equality shorthand. Only meaningful when the primary `call:` is
/// bound to `list` — validated at codegen, not parse time.
#[derive(Debug, Clone)]
pub struct FilterMapping {
    pub field: String,
    pub operator: FilterOperator,
    pub source: FilterSource,
}

/// The comparison of a `filters {}` line. Maps 1:1 onto `common`'s
/// `StoragePredicateOperator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    /// `field: x`, `field = x`, `field == x`
    Eq,
    /// `field != x`
    NotEq,
    Gt,
    Gte,
    Lt,
    Lte,
    /// `field in [a, b]` / `field in input.list`
    In,
    /// `field not in [...]`
    NotIn,
    /// `field like "a%"` (case-sensitive SQL pattern)
    Like,
    /// `field ilike "a%"` (case-insensitive SQL pattern)
    ILike,
    /// `field contains x` — case-insensitive substring match.
    Contains,
}

impl FilterOperator {
    /// The `common::StoragePredicateOperator` variant name.
    pub fn storage_variant(&self) -> &'static str {
        match self {
            FilterOperator::Eq => "Eq",
            FilterOperator::NotEq => "NotEq",
            FilterOperator::Gt => "Gt",
            FilterOperator::Gte => "Gte",
            FilterOperator::Lt => "Lt",
            FilterOperator::Lte => "Lte",
            FilterOperator::In => "In",
            FilterOperator::NotIn => "NotIn",
            FilterOperator::Like => "Like",
            // `contains` is sent as ILIKE '%value%'.
            FilterOperator::ILike | FilterOperator::Contains => "ILike",
        }
    }

    pub fn is_set(&self) -> bool {
        matches!(self, FilterOperator::In | FilterOperator::NotIn)
    }

    pub fn is_pattern(&self) -> bool {
        matches!(self, FilterOperator::Like | FilterOperator::ILike | FilterOperator::Contains)
    }
}

/// The right-hand side of a `filters {}` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterSource {
    /// `input.<field>` — one of the use case's own input fields.
    Input(String),
    /// A fixed value: a quoted string, or a bare word such as `true` or
    /// `42`. Compared as text, like every other predicate value.
    Literal(String),
    /// `[a, b, c]` — fixed values for `in` / `not in`.
    List(Vec<String>),
    /// `now()` — the current UTC time when the request runs.
    Now,
    /// `today()` — the current UTC date when the request runs.
    Today,
}

impl FilterMapping {
    /// The input field this filter reads, if any.
    pub fn input_field(&self) -> Option<&str> {
        match &self.source {
            FilterSource::Input(f) => Some(f),
            _ => None,
        }
    }
}

/// One `field [asc|desc]` entry of a use case's `order_by:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBy {
    pub field: String,
    pub descending: bool,
}

/// `code { file: "./logic/x.rs" infra: A infra: B errors { E } deps { c: "1" } }`
/// — the body of `execute` is read verbatim from `file` (relative to the
/// `.tr` file's working directory) instead of being generated. Every
/// `infra:` becomes a field on the use case struct named after the infra
/// (snake_case), `errors {}` adds extra `Variant(String)` members to the
/// use case's error enum, and `deps {}` adds crates to the generated
/// `Cargo.toml`.
#[derive(Debug, Clone, Default)]
pub struct CodeSpec {
    pub file: String,
    pub infras: Vec<String>,
    pub errors: Vec<String>,
    pub deps: Vec<(String, String)>,
    /// `include: "./logic/shared.rs"` (repeatable) — Rust files copied
    /// once into the crate's `src/shared/` and visible from every `code {}`
    /// body (their `pub` items are re-exported at the crate root).
    pub includes: Vec<String>,
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
    /// `owner: <field>` — the primary call's entity field holding the
    /// owning user's id. Stamped on create/update, filtered on list and
    /// checked on find_one/update_one/delete (a row owned by someone else
    /// reads as `NotFound`).
    pub owner: Option<String>,
    /// Mutually exclusive with `calls`/`ai`/`blob`: a hand-written body.
    pub code: Option<CodeSpec>,
    /// `order_by: due asc, priority desc` — sort order of a `list` primary
    /// call (`id` always breaks ties).
    pub order_by: Vec<OrderBy>,
    /// `transaction: true` — every `call:` runs in one database
    /// transaction (all `psql`, all on the same database).
    pub transaction: bool,
}

#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub name: String,
    pub aliased_type: AllowedType,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AllowedType {
    #[default]
    String,
    Uuid,
    Int,
    Float,
    Decimal,
    Bool,
    DateTime,
    /// `Option<T>` — a nullable field.
    Option(Box<AllowedType>),
    /// `List<T>` — a list of values (`Vec<T>`), stored as `JSONB`.
    List(Box<AllowedType>),
    /// `Json` — any JSON value (`serde_json::Value`), stored as `JSONB`.
    Json,
    /// `Date` — a calendar date without time or zone (`chrono::NaiveDate`).
    Date,
    /// `Time` — a wall-clock time without date or zone (`chrono::NaiveTime`).
    Time,
    /// `Regex<"pattern">` or `Regex<preset>` — a `String` validated
    /// against the pattern whenever it is decoded from JSON.
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
            AllowedType::Bool => "bool".to_string(),
            AllowedType::DateTime => "date_time".to_string(),
            AllowedType::Option(inner) => inner.to_file_name(),
            AllowedType::List(inner) => inner.to_file_name(),
            AllowedType::Json => "json".to_string(),
            AllowedType::Date => "date".to_string(),
            AllowedType::Time => "time".to_string(),
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

/// `enum TaskStatus { todo doing done }` — a closed set of values.
/// Each variant is written exactly as it appears on the wire (JSON and the
/// database column); the generated Rust variant is its UpperCamelCase form.
#[derive(Debug, Clone)]
pub struct EnumDeclaration {
    pub name: String,
    pub variants: Vec<String>,
    pub export: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TypeDeclaration {
    TypeAlias(TypeAlias),
    Struct(StructDeclaration),
    Enum(EnumDeclaration),
}

impl TypeDeclaration {
    pub fn export_name(&self) -> Option<&str> {
        match self {
            TypeDeclaration::Struct(s) => s.export.as_deref(),
            TypeDeclaration::Enum(e) => e.export.as_deref(),
            TypeDeclaration::TypeAlias(_) => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            TypeDeclaration::Struct(s) => &s.name,
            TypeDeclaration::Enum(e) => &e.name,
            TypeDeclaration::TypeAlias(a) => &a.name,
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

/// One `"<cron>" => UseCase` line of a `schedule {}` block.
#[derive(Debug, Clone)]
pub struct ScheduleJob {
    /// Five-field cron expression: minute hour day-of-month month day-of-week.
    pub cron: String,
    pub use_case: String,
    /// Overrides the schedule's own `tz` for this job.
    pub tz: Option<String>,
}

/// `schedule Name { tz: "..." user: 1 "<cron>" => UseCase ... }`: runs use
/// cases on a timer inside the app that lists `Name` in its `run {}`
/// block. Each run calls `execute(user, Default::default())`.
#[derive(Debug, Clone)]
pub struct ScheduleDeclaration {
    pub name: String,
    /// IANA time zone the cron expressions are read in (UTC by default).
    pub tz: Option<String>,
    /// The user id scheduled runs act as (`0` by default).
    pub user: i64,
    pub jobs: Vec<ScheduleJob>,
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
    Schedule(ScheduleDeclaration),
}
