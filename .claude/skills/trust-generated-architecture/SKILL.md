---
name: trust-generated-architecture
description: "Use when: generating, reviewing, hand-writing, or debugging code produced by the trust compiler (any file under out/**, or when editing src/generator/** or templates/**/*.tera that produce it) — i.e. anything about what a compiled `.tr` app's Rust crate must contain and how its layers wire together. Trigger phrases: generated crate, out/ directory, hexagonal layers, domains/applications/infrastructures/adapters, use_case codegen, psql storage codegen, http/grpc adapter codegen, main.rs wiring, workspace Cargo.toml members, grpc_client infra, load_balancer, plugin codegen. Not for trust's own lexer/parser/AST internals (only what they emit)."
---

# Trust-generated crate architecture

`trust` compiles `.tr` DSL files into a Cargo **workspace** (`out/Cargo.toml`,
`[workspace] members = [...]`) containing one independently-deployable,
fully self-contained Rust binary crate **per `app { run { ... } }` block**
in the source file. Every generated crate follows the exact same fixed
hexagonal layout — this file documents that contract so generated (or
hand-written-to-match) code stays structurally correct.

Ground truth for everything below: `out/main/` (a real generated example
in this repo) and `templates/**/*.tera` (the Tera templates that produce
it — see CONTRIBUTING.md's "Code generation templates" section: generated
*text* always lives in a `.tera` file, never in `format!`/`push_str`
chains in the Rust generator).

## Workspace level

- `out/Cargo.toml`: `[workspace] resolver = "2"`, one member per `app`,
  plus a `[profile.release]` tuned for runtime speed (`lto = "fat"`,
  `codegen-units = 1`, `opt-level = 3`)
  block, directory-named `snake_case(app_name)` (e.g. `app PublicHttpBackend`
  → `out/public_http_backend/`). Single-app files still produce a
  workspace of one member.
- **No shared internal crate between apps.** Each app's crate contains a
  full, filtered copy of every `type`/`use_case`/`infra` it actually
  references — not a reference to a shared crate. Only two things are
  genuinely external and shared: the `common` crate (always a git
  dependency on Trust's own `runtime/common`,
  `https://github.com/trust-env/trust.git`) and any
  `import X from "git@..."` use-case source repos. Confirmed by scenario 04
  (multi-app-workspace): `http_api`'s crate has `CreateTaskUseCase`,
  `grpc_worker`'s does not, even though both share `TaskInfra`/`Task`/
  `GetTaskUseCase` — each got its own copy, independently filtered.

## Per-crate layout (one `app`'s directory)

```
{app}/
  Cargo.toml
  build.rs                  # only if a grpc adapter or grpc_client infra exists
  proto/*.proto             # only if a grpc adapter or grpc_client infra exists
  src/
    main.rs                 # wiring: env/static config -> storage -> use case -> adapter -> tokio::spawn
    lib.rs                  # mod domains; mod infrastructures; mod applications; mod adapters; (+ mod plugins;)
    domains.rs               + domains/{type}.rs           (one file per `type` block)
    infrastructures.rs        + infrastructures/storage.rs
                                 + storage/psql.rs           + storage/psql/{entity}.rs
                                 + storage/grpc_client.rs     + storage/grpc_client/{entity}.rs
    applications.rs           + applications/{use_case}.rs  (one file per `use_case` block)
    adapters.rs                + adapters/http.rs            + http/{presenter}.rs + http/{presenter}/{route}.rs
                                 + adapters/grpc.rs            + grpc/{presenter}.rs
                                 + adapters/http/load_balancer.rs   (per `load_balancer {}`, static text — no `.tera`)
    plugins.rs                 + plugins/{name}.rs          (only if a plugin keyword was used)
```

### The barrel-file pattern (universal, every directory level)

Every `*.rs` file that indexes a sibling directory — `lib.rs`, `domains.rs`,
`applications.rs`, `adapters.rs`, `adapters/http.rs`,
`infrastructures/storage.rs`, `infrastructures/storage/psql.rs`, etc. — is
generated from the single `templates/file/mod_file.rs.tera` template and is
**always exactly**:

```rust
mod a;
mod b;

pub use a::*;
pub use b::*;
```

Never hand-roll a different shape here (no re-exports with renaming, no
`pub mod`). This is what lets every layer address every other layer's
types with a flat `crate::{Thing}` import regardless of nesting depth.

## Layer contracts

### `domains/{type}.rs` — plain data

One file per `type` block. A bare struct:
`#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]`, plus
`sqlx::FromRow` if the type is used as a `psql` entity anywhere. No
methods, no `impl` blocks beyond derives — domain types are pure data.

### `infrastructures/storage/psql/{entity}.rs` — psql-backed `Storage<T,E>`

For each `infra { storage { psql { entity: X } } }`:
- `{Entity}StorageError` enum: `ConversionError(String)`, `DatabaseError(String)`,
  plus `impl From<sqlx::Error> for {Entity}StorageError` (→ `DatabaseError`).
- `{Entity}Storage: common::Storage<{Entity}, {Entity}StorageError> + Send + Sync` —
  a trait (only adding the two `*_owned` methods below), not a concrete type, so use cases and adapters can be
  generic over "any storage for this entity" rather than this one struct.
- `Psql{Entity}Storage { db: SQLConnector }` (`#[derive(Clone)]`), `::new(db)`,
  and `impl Storage<{Entity}, {Entity}StorageError> for Psql{Entity}Storage`
  implementing all **8** `common::Storage<T,E>` methods via raw
  `sqlx::query_as::<_, {Entity}>(...)` against a table named after the
  pluralized snake_case entity: `save_one`, `save_many` (one
  `INSERT ... SELECT * FROM UNNEST($1::T[], ...)`), `find_one`, `find_many`
  (`WHERE id = ANY($1)`), `list` (dynamic `WHERE` built from
  `StorageListQuery.predicates` — column names whitelisted, the text
  *parameter* cast to the column type so indexes stay usable — then
  `ORDER BY id`, clamped `LIMIT`/`OFFSET`), `update_one`, `delete`,
  `delete_many`.
- `{Entity}Storage` also declares `update_one_owned`/`delete_owned`
  (owner-checked single-statement writes used by `owner:` use cases);
  `impl {Entity}Storage for Psql{Entity}Storage` implements them, the
  `grpc_client` variant stubs them as unsupported.
- `sqlx::query_as` is chosen deliberately over compile-time-checked
  `sqlx::query!` — runtime validation, no live DB needed to `cargo build`
  the generated project or run `generator_*`/most `e2e_*` tests
  (CONTRIBUTING.md).

### `infrastructures/storage/grpc_client/{entity}.rs` — remote `Storage<T,E>`

For each `infra { storage { grpc_client { service, on, entity, methods {...} } } }`
— same `Storage<T,E>` trait, different backend, so a `use_case { call: X.find_one }`
cannot tell the difference:
- Own duplicate `.proto` + own `build.rs`/`tonic_build` step — **not** a
  shared crate with the target adapter's server side, even when both apps
  live in the same workspace (proven by scenario 05).
- `Grpc{Entity}Client { client: proto::...Client<Channel> }`, `::connect(addr)`.
- Only the `Storage` verbs explicitly bound under `methods { verb: Rpc }`
  get a real implementation (today, effectively just `find_one`); every
  other verb still compiles (the trait requires all 8) but renders as an
  `unsupported_method` stub that returns
  `{Error}::DatabaseError("{verb} is not supported by this grpc_client infra")`.
- `find_one`'s gRPC `NOT_FOUND` status is special-cased to `Ok(None)` —
  matching every other backend's "no such row" contract — not treated as
  an error; every other `tonic::Status` maps to `DatabaseError` via a
  blanket `From<tonic::Status>`.
- The target address is a **static string literal** from the `.tr` file,
  baked directly into `main.rs` — never an env var (that's what
  distinguishes it from `psql`'s `{INFRA}_DB_URL` in `main_file.rs`'s
  `InfraKind::Psql` vs `InfraKind::GrpcClient { target_address }`).

### `applications/{use_case}.rs` — one `use_case` block, one file

Fixed shape, in this order:
1. `{Name}Input` (`#[derive(Debug, Clone, serde::Deserialize)]`) — mirrors
   the `input:` type, or is synthesized from the storage call's
   parameters when `input:` is omitted (e.g. `GetTaskUseCase { call:
   TaskInfra.find_one }` with no declared input/output).
2. `impl From<{Name}Input> for {Entity}` — only when the use case
   *creates* an entity (`save_one`/`update_one`-style calls).
3. `{Name}Output` (`#[derive(serde::Serialize)]`) + `impl From<{Entity}> for {Name}Output`.
4. `{Name}Error` enum — always at least `NotFound`, `StorageError(String)`,
   `InternalError(String)`; any `errors { Source -> Target }` mapping in
   the `.tr` adds/renames variants and drives the
   `impl From<{Storage}Error> for {Name}Error` match arms.
5. `{Name}Trait: UseCase<{Name}Input, {Name}Output, {Name}Error>` — a
   marker trait every downstream presenter is generic over instead of the
   concrete `{Name}<S0>` struct.
6. `{Name}<S0>` (`#[derive(Clone)]`, one type param **per storage/dependency
   it calls**, e.g. `S0: {Entity}Storage`) holding one field per dependency,
   `::new(...)`.
7. `impl<S0> UseCase<Input, Output, Error> for {Name}<S0>`:
   `async fn execute(&self, _user: i64, input: Input) -> Result<Output, Error>`
   — calls the bound storage method, `.map(Output::from).map_err(Error::from)`
   (or `.ok_or(Error::NotFound)` for a `find_one`-backed read — see
   `templates/file/use_case/call_read.rs.tera` vs `call_simple.rs.tera`).
8. `impl<S0> {Name}Trait for {Name}<S0> {}`.

`ai { infra, prompt }` use cases (`templates/file/ai_use_case/*.tera`)
produce the **same** Input/Output/Error/Trait/struct/impl shape (steps
1–8) — the only difference is `execute`'s body is authored by an LLM at
compile time instead of a deterministic template body, and the struct has
no type param at all when `has_infra` is false (a pure-function use case).
Never assume an `ai` use case's generated *surface* differs from a normal
one — only the method body's provenance does.

### `adapters/http/{presenter}.rs` + `{presenter}/{route}.rs` — HTTP adapter

Per `adapter { http { on, routes {...} } }`:
- Presenter struct generic over **one type param per use case it routes**
  (`C0: FooTrait + Send + Sync + 'static, C1: BarTrait + ...`), one field
  per use case, `::new(...)`, and
  `impl HttpPresenter for {Presenter}<C0,...> { fn router(self: Arc<Self>) -> Router }`
  wiring one `.route(path, method(Self::handler))` per declared route,
  ending `.with_state(self)`.
- One submodule file per route, named `{use_case_snake}_{http_verb}.rs`:
  a private `fn {route}_status(&Error) -> StatusCode` (default
  `NotFound -> 404`, everything else `-> 500`, overridden per-variant by a
  `status { Variant -> code }` block in the `.tr` route), and
  `impl<C0,...> {Presenter}<C0,...> { pub(crate) async fn {route}_handler(...) -> HttpResult<Output> }`
  that extracts input (`Json<Input>` for a body, `Path<Input>` for path
  params, or a split-struct pair — see below — when both are present),
  calls `presenter.{field}.execute(0, input).await`, and maps `Err` to
  `HttpError::new(status, format!("{:?}", error))`.
- When a route needs **both** path params and a JSON body, a
  `split_structs.rs.tera`-generated pair of private structs
  (`{Route}Path` / `{Route}Body`, both `#[derive(serde::Deserialize)]`)
  splits the one DSL `input:` type into what each extractor needs.
- A route with an `auth`/permission requirement gets an
  `auth_prelude.rs.tera` block spliced at the top of its handler: it looks
  up the caller's permission row via the bound infra's `list()` (matching
  `user_id`/`target`/`action` predicates) and returns `403 Forbidden` if
  none is found, **before** the use case executes.
- In `--dev` mode, an `app_docs.rs` handler + `/docs` route serves static
  generated HTML (`templates/dev_docs/*.tera`) documenting every route,
  its fields, and its declared error→status table — purely additive, never
  required for the crate to compile/run in production.

### `adapters/http/load_balancer.rs` — the one static (non-`.tera`) file

Per `load_balancer { targets {...}, on }` inside an `app`. Copied
byte-for-byte from `templates/file/load_balancer.rs` (a plain `.rs` file,
**not** `.tera` — no per-app variables to substitute). Defines
`RoundRobinProxy` (atomic-counter round robin over a fixed
`Vec<String>` of real network addresses) and `load_balancer_router(proxy)`,
which reverse-proxies every request via `reqwest`, streaming bodies both
ways (never buffered whole), stripping hop-specific
headers (`host`, `content-length`, `transfer-encoding`) in both
directions. It is adapter-agnostic — it never constructs or knows about
the backend's presenter type, only its addresses. Do not confuse this with
in-process load balancing: scenario 02 exists specifically to prove targets
are separately-deployed OS processes, not shared tokio tasks.

### `adapters/grpc/{presenter}.rs` — gRPC adapter

Per `adapter { grpc { on, services { rpc ... } } }`:
- `pub mod proto { tonic::include_proto!("{package}"); }` and
  `pub use proto::{Name}Server;`.
- Presenter struct, same generic-over-use-case-trait shape as the HTTP
  presenter.
- `#[tonic::async_trait] impl {Name}Service for {Presenter}<C0,...>`, one
  method per `rpc`: proto request → use case `Input`, `.execute(0, input)`,
  map use case `Error` → `tonic::Status` (`NotFound` → `Status::not_found`,
  everything else → `Status::internal(format!("{:?}", other))`), build the
  proto response from `Output`.

### `proto/*.proto`

One `message` per Input/Output/Entity DTO the adapter touches
(`templates/proto/message.proto.tera`, or `wrapper_message.proto.tera` for
an `Option<Message>`-wrapped field), one `service {Name} { rpc Verb(...) returns (...); }`
block. A `grpc_client` infra generates its **own** proto file
(`{entity}_client.proto`), reusing the target's exact `package`/`service`
names so `tonic::include_proto!` resolves against its own `build.rs`-compiled
copy — never a shared `.proto` between the two sides.

### `plugins/{name}.rs` — plugin-defined constructs

Only present when the `.tr` file uses a top-level keyword registered by a
`--plugin`-loaded `.so`. Contract between `trust` and a plugin (see
`examples/plugins/note_plugin` for a full reference implementation):
- C-ABI, `#[unsafe(no_mangle)] extern "C"` exports:
  `trust_plugin_abi_version() -> u32`, `trust_plugin_register() -> *mut c_char`
  (returns `{"plugin_name","keyword","templates":[{"name","content"}]}`),
  `trust_plugin_parse(raw_body: *const c_char) -> *mut c_char` (plugin's
  own hand-rolled parser for its construct's raw body text — trust never
  interprets it), `trust_plugin_codegen(node_json, host_context_json) -> *mut c_char`
  (returns `{"template_name","context","output_file"}`),
  `trust_plugin_free_string(ptr)`.
- Every call returns a JSON envelope `{"ok": true, "result": ...}` or
  `{"ok": false, "error": "..."}`; every exported function body is wrapped
  in `catch_unwind` (unwinding across an `extern "C"` boundary is UB).
- The plugin owns any memory it allocates (returned strings); trust never
  frees plugin memory except via `trust_plugin_free_string`.
- The rendered file gets written to `src/plugins/{output_file}` and
  registered in `plugins.rs`'s barrel file exactly like every built-in
  layer.

## `main.rs` — the only place concrete types meet

`main.rs` is generated from `templates/file/main/*.tera` and is the single
file allowed to name concrete types and own construction order. Always:
storage → use case → adapter/presenter, each declared adapter/load-balancer
spawned as its own `tokio::spawn`, all task handles `.await??`'d at the
end. Presenters/use cases themselves are never told to construct their own
dependencies — they only ever receive already-built instances via `::new()`.

- `psql` infra connection: `std::env::var("{INFRA}_INFRA_DB_URL")` (shouty
  snake case of the `infra` block's identifier), `.expect(...)`, passed to
  `SQLConnector::connect_with_pool_size(&url, size).await?`, where `size`
  comes from the optional `{INFRA}_DB_POOL_SIZE` env var (default 16 —
  `common`'s plain `connect` caps the pool at 5).
- `grpc_client` infra connection: `Grpc{Entity}Client::connect(target_address)`
  where `target_address` is the literal string from the `.tr` file — no env var.
- `load_balancer`: constructs `RoundRobinProxy::new(vec![...declared targets...])`
  and serves `load_balancer_router(proxy)` — **never** constructs the
  backend presenter itself, even when (as in scenario 02) the backend's
  `adapter {}` is declared in the same `.tr` file. That's what makes the
  balancer prove real cross-process distribution rather than in-process
  fan-out.
- Each app's own `main.rs` only spawns tasks for what its own
  `app { run { ... } }` block lists — scenario 04/05 confirm per-app
  filtering extends all the way into `main.rs`, not just which files exist.

## `Cargo.toml` / `build.rs`

`Cargo.toml` (`templates/cargo_toml.toml.tera`) dependencies are additive,
driven by what the crate actually uses — not a fixed superset:
- Always: `serde` (+`derive`), `common` (git dep), `dotenvy`, `tokio` (`full`).
- `psql` infra present → `sqlx` (`runtime-tokio`, `tls-rustls`, `postgres`).
- `http` adapter present → `axum`.
- `load_balancer` present → `reqwest` with `stream` (the proxy's HTTP client).
- `grpc` adapter or `grpc_client` infra present → `tonic`, `prost`, and
  `[build-dependencies] protoc-bin-vendored`, `tonic-build`, plus a
  `build.rs` that vendors `protoc` (`std::env::set_var("PROTOC", ...)`)
  and calls `tonic_build::compile_protos("proto/{file}.proto")` — one call
  per proto file the crate owns (its own adapter's, and/or its own
  `grpc_client` infra's duplicate).

## DSL construct → generated artifact, quick reference

| `.tr` construct | Generated artifact |
|---|---|
| `import X from "git@..."` | `common = { git = "..." }` (or other named) dependency in `Cargo.toml`; no Rust file of its own |
| `type T { ... }` | `domains/{t}.rs` |
| `infra I { storage { psql { entity } } }` | `infrastructures/storage/psql/{entity}.rs` + env-var-driven connection in `main.rs` |
| `infra I { storage { grpc_client { ... } } }` | `infrastructures/storage/grpc_client/{entity}.rs` + own `proto/*.proto` + own `build.rs` + static-address connection in `main.rs` |
| `use_case U { call: I.method }` | `applications/{u}.rs` (deterministic body) |
| `use_case U { ai { infra, prompt } }` | `applications/{u}.rs` (LLM-authored `execute` body, same surrounding shape) |
| `adapter A { http { routes {...} } }` | `adapters/http/{a}.rs` + one `{a}/{route}.rs` per route |
| `adapter A { grpc { services {...} } }` | `adapters/grpc/{a}.rs` + `proto/{a}.proto` + `build.rs` entry |
| `app Name: run { AdapterX, load_balancer L {...} }` | one workspace member `{name}/`, its own filtered copy of every referenced layer, its own `main.rs` |
| plugin-registered keyword | `plugins/{name}.rs` + entry in `plugins.rs`, per the plugin's own template |

## Invariants worth defending in review

1. Barrel files (`mod x; pub use x::*;`) never gain hand-written logic —
   if a layer needs shared logic, it goes in the layer's own generated
   file, not the aggregator.
2. Presenters and use-case structs are generic over **trait bounds**
   (`{Name}Trait`, `{Entity}Storage`), never over a concrete type — this is
   what lets `main.rs` be the only file that names concrete types.
3. Nothing outside `main.rs` constructs its own dependencies; everything
   receives them via `::new(...)`.
4. No app's crate references another app's crate directly — cross-app
   calls only ever happen over the network (`grpc_client`), each side with
   its own duplicate proto/types, never a shared internal crate.
5. Generated-file text changes belong in `templates/**/*.tera`; Rust-side
   generator code only resolves *data* and builds the `tera::Context`
   (CONTRIBUTING.md — this is a hard project rule, not a style preference).
