# Conversions

This page is the exhaustive reference for every transformation Trust performs
when turning `.tr` source into generated Rust. If you've ever wondered "what
Rust code will this line actually produce," this is the page to check.

Everything here was verified directly against `src/` and `templates/` — file
references are given so you can re-verify against a newer version of the
compiler if behavior seems to have changed.

## 1. Scalar type mapping

| `.tr` type | Generated Rust type | Notes |
|---|---|---|
| `String` | `String` | — |
| `Uuid` | `Uuid` | Pulls in `use uuid::Uuid;` and a `uuid = "1"` dependency with the `serde` feature. If the type is also used as a storage `entity:`, the crate's `sqlx` dependency additionally gets the `uuid` feature. |
| `Int` | `i64` | Not `i32` — always 64-bit. |
| `Regex<pattern>` | `String` | **The pattern is discarded.** `label: Regex<slug>` and `label: String` generate byte-identical code — no runtime validator, no regex crate dependency, nothing. Treat `Regex<...>` as a documentation-only annotation today (see [limitations.md](limitations.md)). |
| a `DefinedType` (another `type`/struct name) | the matching generated struct | Resolved against everything already registered in the compilation unit (own file + successfully imported types). Referencing a name that doesn't resolve is a **codegen-time** error ("Undefined type reference: `{name}`"), not a parse-time one — a typo'd type name will lex and parse fine and only fail when you actually compile. |

Every generated struct field is unconditionally `pub` — there is no field
visibility control in the DSL (`templates/types.rs.tera`).

### Derives

Every generated domain struct gets:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
```

and, additionally, `sqlx::FromRow` if the type is used as a storage
`entity:`. This is not configurable from `.tr` source.

## 2. Naming conversions

Trust only ever applies **snake_case** (via `heck::ToSnakeCase`) — there is
no PascalCase/CamelCase normalization anywhere. Struct, type, use_case,
adapter, and app names are emitted **exactly as written** in your `.tr`
file; if you name a type `myTask`, the generated struct is `pub struct
myTask`. Capitalization discipline is entirely your responsibility.

| `.tr` identifier | Conversion | Becomes | Where |
|---|---|---|---|
| A `type`/struct name | snake_case | file `src/domains/{name}.rs` | domain module path |
| A `DefinedType` reference | snake_case | lookup key (`AllowedType::to_file_name()`) used to resolve/import the module | type resolution |
| An `adapter` name | snake_case | file under `src/adapters/{http,grpc}/{name}.rs` and its module name | adapter module path |
| A route's/rpc's `use_case:` reference | snake_case | struct field name holding that use case on the generated adapter struct | adapter struct field |
| A `use_case` name | snake_case | file `src/applications/{name}.rs`, module name, and `{Name}Error` prefix (kept as-written for the `Error` suffix — only the file/module path is snake_cased) | use case module path |
| A `call`'s target infra name | snake_case | struct field name for that storage dependency on the use-case struct | use-case struct field |
| An entity name, for a `list`/`find_many`-shaped output | snake_case **+ trailing `s`** | output struct's collection field name — entity `Task` → field `tasks` | output struct shape |
| An entity name, for `psql` storage | snake_case **+ trailing `s`** | the SQL table name Trust queries — entity `Task` → table `tasks` | generated SQL |
| An `app` name | snake_case | crate/output folder name under `out/`, and the Cargo workspace member/package name | workspace layout |

There is no pluralization rule beyond a flat trailing `s` — an entity named
`Datum` becomes table/field `datums`, not `data`.

## 3. Storage trait shape (`psql`)

For `infra X { storage { psql { entity: Task } } }`, Trust generates
`src/infrastructures/storage/psql/task.rs` containing:

- **Error enum** `PsqlTaskStorageError` with exactly two variants —
  `ConversionError(String)` and `DatabaseError(String)` — plus a `From<sqlx::Error>`
  impl that always maps into `DatabaseError`. This same two-variant shape is
  what every `errors { Source -> Target }` mapping maps *from* (see
  [Error enum generation](#4-error-enum-generation) below) — `Source` can
  only ever be `ConversionError` or `DatabaseError`, regardless of what
  storage backend you're using.
- **Struct** `pub struct PsqlTaskStorage { db: SQLConnector }`, built via
  `PsqlTaskStorage::new(db: SQLConnector)`.
- An implementation of the shared, generic `Storage<Task, PsqlTaskStorageError>`
  trait (defined once in the external `common` crate, not regenerated per
  entity):

| Method | Signature |
|---|---|
| `save_one` | `async fn save_one(data: Task) -> Result<Task, PsqlTaskStorageError>` |
| `save_many` | `async fn save_many(data: Vec<Task>) -> Result<Vec<Task>, PsqlTaskStorageError>` |
| `find_one` | `async fn find_one(id: EntityId) -> Result<Option<Task>, PsqlTaskStorageError>` |
| `find_many` | `async fn find_many(ids: Vec<EntityId>) -> Result<Vec<Task>, PsqlTaskStorageError>` |
| `list` | `async fn list(query: StorageListQuery) -> Result<Vec<Task>, PsqlTaskStorageError>` |
| `update_one` | `async fn update_one(id: EntityId, data: Task) -> Result<Task, PsqlTaskStorageError>` |
| `delete` | `async fn delete(id: EntityId) -> Result<Task, PsqlTaskStorageError>` |
| `delete_many` | `async fn delete_many(ids: Vec<EntityId>) -> Result<Vec<Task>, PsqlTaskStorageError>` |

`grpc_client` storage implements the exact same `Storage<Entity, Error>`
trait shape (so `call:` syntax on a use case is identical regardless of
backend) — but today only `find_one` actually has real codegen; every other
method mapped in a `grpc_client`'s `methods {}` block is accepted by the
parser and rejected at codegen time (see [limitations.md](limitations.md)).

## 4. Error enum generation

For every `use_case Name { ... }`, Trust generates:

```rust
#[derive(Debug)]
pub enum NameError {
    NotFound,
    StorageError(String),
    // + one variant per unique target named across every `errors { Source -> Target }`
    //   mapping on any of this use case's `call`s, deduplicated
}
```

Then, **per distinct storage error type** any of the use case's `call`s
touch, an `impl From<StorageErrorType> for NameError`:

- **No `errors {}` block on that call**: everything falls back to
  `NameError::StorageError(format!("{:?}", error))`.
- **`errors { Source -> Target }` present**: each of the two fixed source
  variants (`ConversionError`, `DatabaseError`) you *do* map is translated to
  your named target. Any source variant you *don't* mention still falls
  through to `StorageError` — mapping is additive/partial, not all-or-nothing.
  - `Target("literal message")` → the generated arm **ignores the real
    underlying error** and always constructs `Target("literal message".to_string())`.
  - `Target` with no literal → the real underlying error string is threaded
    through: `Target(msg)`.
- Duplicate-mapping the same source variant twice within one `call`'s
  `errors {}` block is a **parse-time** error.

`NotFound` and `StorageError` always exist on every use-case error enum,
whether or not you reference them — and (see [keywords.md](keywords.md#status))
their eventual HTTP status codes are hardcoded and not overridable.

## 5. `use_case` multi-`call` semantics

A `use_case` may declare more than one `call:`/`call: { ... }` entry. This
isn't documented syntax sugar — it has real, specific semantics:

- Calls are kept in **declaration order**; the **last** call is the "primary"
  one and alone determines the generated `{Name}Output` shape (and whether
  the use case behaves like a `find_one`/`list`/etc. for `NotFound` purposes).
  Earlier calls run first as shared side effects/guards.
- Every call shares the **same** `{Name}Input` struct — there's no way to
  give an earlier call a different input shape.
- If the same infra target appears in more than one `call`, it's deduplicated
  to a single generic parameter/struct field — you don't get two separate
  handles to the same infra.
- Each `call`, even when sharing an infra target with another `call` in the
  same use case, keeps its **own independent** `errors {}` mapping.
- Only calls bound to `save_one` or `update_one` get a `From<Input> for
  Entity` conversion generated (`find_one`/`find_many`/`list`/`delete`/`delete_many`
  don't need one, since they don't construct a new entity from the input).
- Certain storage methods implicitly require certain input fields — the
  compiler doesn't currently document this on the input struct itself, but
  the shape is fixed: `find_one`/`update_one`/`delete` expect an `id` field;
  `list` expects `limit`+`offset`; `save_one` expects plain entity data
  (no `id`).

## 6. HTTP path parameter conversion

- A path segment `:name` in a `.tr` route is extracted (`path_params_of`)
  and rewritten to axum 0.8's own path-param syntax `{name}` (`to_axum_path`)
  when generating the router — `"/tasks/:id"` becomes `.route("/tasks/{id}", ...)`.
- If the route's `input` type has **only** fields that are path params (or
  no fields at all), the handler extracts the whole input struct directly:
  `Path(input): Path<TaskIdInput>`.
- If the input type mixes path-param fields with other (body) fields, Trust
  splits it into **two** generated structs and extracts them separately —
  `Path(path): Path<...PathFields>` and `Json(body): Json<...BodyFields>` —
  then reassembles them into the original input type before calling the use
  case. A field is routed to the path extractor purely by matching its name
  against the `:name` segments in the route path.

## 7. Address string / env var interpolation

Any `on:` address on an `adapter` (`http` or `grpc`) or a `load_balancer`'s
own listen address (**not** its `targets` — see the important note below)
may contain `${VAR}` placeholders. Trust compiles the string into a runtime
lookup instead of a fixed literal:

| `.tr` string | Generated Rust |
|---|---|
| `"postgres://localhost"` (no placeholders) | `"postgres://localhost".to_string()` |
| `"${HOST}"` | `format!("{}", std::env::var("HOST").expect("HOST environment variable not set"))` |
| `"${HOST}:${PORT}"` | `format!("{}:{}", std::env::var("HOST").expect("HOST environment variable not set"), std::env::var("PORT").expect("PORT environment variable not set"))` |

Literal `{`/`}` characters next to a placeholder are escaped to `{{`/`}}` so
the generated `format!` call itself stays valid.

> **Important — `targets` are NOT interpolated.** A `load_balancer`'s
> `targets { "10.0.0.1:8080" ... }` entries are always compiled as plain
> static string literals, even if they contain `${...}`-looking text. Only
> the load balancer's *own* `on:` (the address it listens on) and adapter
> `on:` fields get env var interpolation. If you need the backend fleet to
> be configurable, you currently have to recompile — this is a real,
> intentional asymmetry, not a bug, but it's easy to assume otherwise.

In `--dev` mode, every `${VAR}` actually found this way (across HTTP
adapters, gRPC adapters, and load-balancer `on:` — again, never `targets`)
is also written into that app's generated `.env` with a sniffed default; see
[cli.md](cli.md#dev-mode---dev) for the exact rule.

## 8. Proto / gRPC message conversion

- Struct fields become proto3 message fields, numbered 1-based in
  declaration order (`templates/proto/message.proto.tera`).
- A CRUD-style use case's entity output gets wrapped in a single-field
  "wrapper message" whose field is the entity's own message type — e.g.
  `message CreateUserOutput { User user = 1; }` (`wrapper_message.proto.tera`)
  — rather than the entity's fields being duplicated inline.
- **Field type support for gRPC is much narrower than HTTP/psql**: only
  `String` (→ `string`) and `Int`/`i64` (→ `int64`) fields are supported.
  `Uuid`, list-shaped (`Vec<...>`) outputs, and nested struct fields are a
  hard **generation-time error** ("gRPC doesn't support field type '...' yet
  — only String and Int fields are supported"), not a parse-time one and not
  currently called out in [limitations.md](limitations.md) at the same level
  of prominence as the `grpc_client` CRUD-verb limitation — but it's just as
  real. Plan gRPC-exposed types accordingly.
- gRPC error mapping is **not customizable at all** — there is no `status {}`
  equivalent for `rpc`. Codegen hardcodes: your enum's `NotFound` →
  `Status::not_found(...)`, and literally everything else → `Status::internal(...)`.

## 9. Dependency conversion (what adds what to `Cargo.toml`)

Each per-crate `Cargo.toml` is assembled by conditionally requiring
dependencies as the AST is walked — nothing is added speculatively:

| `.tr` feature used anywhere in the app | Dependencies added |
|---|---|
| any `use_case`/`infra` at all | `common` (git), `serde` (`derive`), `tokio` (`full`), `dotenvy` |
| a type field of `Uuid`, or `psql` storage | `uuid` (`serde` feature; `+uuid` feature on `sqlx` if it's an entity) |
| `psql` storage | `sqlx = "0.9.0"` |
| any `http` adapter | `axum = "0.8"` |
| any route with `auth {}` | `auth` (git) |
| any `grpc` adapter, or `grpc_client` storage | `tonic = "0.12"`, `prost = "0.13"` (plus `tonic-build`/`protoc-bin-vendored` as **build**-dependencies, adapter side only) |
| `load_balancer {}` | `axum = "0.8"`, `reqwest = "0.12"` |
| `ai {}` | **nothing.** The Anthropic call happens once, at *compile time*, inside the `trust` binary itself. The generated project never talks to any AI provider at runtime and gets no extra dependency for it. |

## 10. Route/`rpc` `input`/`output` are a cross-check, not the source of truth

This is easy to get backwards. On an HTTP `routes { ... }` entry or a gRPC
`rpc { ... }`, the `input:`/`output:` fields (or `input { body: T }`) are
**not** what defines the generated request/response struct or proto
message. The actual shape always comes from the **use_case's own** `input`/
`output` (explicit if declared, otherwise inferred from its `call` chain —
e.g. from the target storage entity).

What the route/rpc's `input:`/`output:` actually does:

- If the referenced use_case has **no** explicit `input`/`output` of its own
  (fully inferred), whatever you write on the route/rpc is accepted **and
  silently ignored** — verified directly: `rpc GetTask { input: GetTaskRequest
  ... }` where `GetTaskRequest` isn't even a declared type anywhere in the
  file compiles cleanly, and the generated proto/struct names are always
  `GetTaskUseCaseInput`/`GetTaskUseCaseOutput` (derived from the use case's
  own name), never `GetTaskRequest`.
- If the use case **does** declare an explicit `input`/`output`, then a
  route/rpc that also declares one must name the **exact same type**, or
  it's a hard codegen error: `"HTTP route '...': declared input does not
  match use_case '...' input"` (same message shape for `output`, and for
  `rpc` with "gRPC rpc" instead of "HTTP route").

So in practice: `input`/`output` on a route or `rpc` are optional,
self-documenting consistency checks against the use case they expose —
useful for making a route's contract explicit and catching drift if the
use case's signature changes, but never a way to reshape what's generated
independently of the use case.

## 11. Other conversions worth knowing

- **`export Name`** only affects whether another `.tr` file's `import` can
  find this type by name (`generator/import.rs`'s `find_exported_type`
  matches against it). It has **no effect on the type's own generated
  code** — no different derives, no visibility change.
- **Git-style `import ... from "git@..."`** parses successfully
  (`ModuleKind::Git`) but hard-errors the moment codegen tries to load it:
  `"Git imports are not supported yet: {url}"`. Only local path imports
  (`ModuleKind::Path`) actually resolve.
- **Generated module layout is fixed** and always snake_case:
  `src/domains/`, `src/infrastructures/storage/{psql,grpc_client}/`,
  `src/applications/`, `src/adapters/{http,grpc}/`, and `src/plugins/` (only
  if a plugin block was used). Each level's `mod.rs`/`lib.rs` only declares
  submodules that were actually populated — nothing is stubbed out empty.
