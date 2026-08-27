# Current Limitations

## External Rust Type Reuse

- Directly importing an existing Rust `.rs` type/class into Trust DSL is not supported.
- Trust imports operate on exported Trust types from `.tr` modules.

## Git Imports

- Git-style module import syntax is recognized by the parser.
- Code generation currently rejects git imports ("Git imports are not
  supported yet") — despite this exact form appearing in the top-level
  README's own DSL example. Only local path imports work.

## `filters` Coverage

- **Equality only.** Every `filters {}` entry becomes a
  `StoragePredicateOperator::Eq` predicate — there's no syntax for
  `>`/`<`/`!=`/`LIKE`/`IN`/etc.
- **AND-only, no OR.** Multiple `filters {}` entries all apply together
  (`WHERE a = X AND b = Y`) — there's no way to express alternatives.
- **Only valid on a `list`-bound primary call.** `find_many` (which also
  takes a collection of some kind — a list of IDs) doesn't accept
  `filters {}`, only `list`.
- **Only on the use case's own `list` predicates**, not composable with a
  route's own query-string parameters or anything outside `input.<field>`
  — the right-hand side must reference an existing input field, full stop.
- No way to filter on a field of a *different* type than the one declared
  on `input` — the guessed-input path always matches the entity field's
  type exactly; an explicit `input` with a mismatched type for that field
  will still compile (the predicate's value is always `.to_string()`'d)
  but may not compare correctly against the stored column depending on
  its actual SQL type.

## `s3` / `blob` Coverage

- Only three operations exist: `presigned_put_url`, `presigned_get_url`,
  `delete`. There's no direct (non-presigned) upload/download through the
  generated backend, no listing, no multipart upload, no object metadata
  read.
- A `blob {}` use case's input/output shape is **always fixed** (`{ key:
  String }` in; `{ url: String }` or `{ key: String }` out) — an explicit
  `input`/`output` declaration is a hard error, not a narrowing/override.
- No `errors {}` customization for `blob {}` use cases — same fixed
  `{NotFound, StorageError(String)}` enum `ai {}` uses. `NotFound` is never
  actually constructed.
- `s3` storage cannot be exposed over gRPC (`rpc`) — `blob {}` use cases
  are HTTP-adapter only in practice today (nothing parser-level stops
  wiring one to a `rpc`, but the concrete, non-generic storage field this
  generates has not been exercised against gRPC's own codegen path).
- Presigned URL expiry is hardcoded to 900 seconds (15 minutes) — not
  configurable from `.tr` source.
- Region is always hardcoded to the placeholder `"us-east-1"` — fine for
  MinIO and most S3-compatible services (which ignore it), but there's no
  way to target a real AWS S3 bucket in a region-sensitive way today.
- `entity:`'s own fields are never read by the generated storage code — see
  [`entity`](keywords.md#entity)'s doc comment. There's no way to store or
  validate object metadata (content type, size, etc.) beyond the bucket
  key itself.
- Endpoint/access key/secret key are always read from three fixed env var
  names derived from the infra name — there's no way to override the
  derived name or supply them any other way.

## `psql`/`grpc_client` Entity IDs Are Always `i64`

- The shared `Storage<T, E>` trait (in the external `common` crate) defines
  `find_one`/`update_one`/`delete`/`find_many`/`delete_many` in terms of a
  fixed `EntityId` type — concretely `i64`, not a generic parameter tied to
  whatever type the entity's own `id` field declares. `s3`/blob entities are
  unaffected — they never touch `EntityId` at all (see `S3InfraService`'s
  doc comment).
- **Caught at `trust` compile time**, not left to fail only once the
  generated crate is built: a `use_case` whose primary call is
  `find_one`/`update_one`/`delete` against an entity whose `id` field isn't
  `Int` is a clear codegen error (`requires entity '...''s 'id' field to be
  'Int'`). `save_one`/`list` are unaffected — neither passes a standalone
  `EntityId`, so a non-`Int` id is fine there (e.g. an entity you only ever
  create/list, never look up by id).

## Reusing One `use_case` Across Multiple Routes With the Same HTTP Method

- A route's generated adapter file/handler function is named
  `{use_case_name}_{http_method}` (e.g. `update_user_use_case_put`) — keyed
  by the **(use_case, method)** pair, not by the route's path.
- **Caught at `trust` compile time**: binding the *same* use case to two
  routes on the same HTTP adapter that share an HTTP method (e.g.
  `PUT /users/:id` and `PUT /admin/users/:id/role`, both using
  `UpdateUserUseCase`) is a clear codegen error naming both conflicting
  paths, rather than the duplicate-module error you'd otherwise only see
  once the generated crate is compiled. Different methods on the same use
  case (e.g. `GET`+`POST` both hitting `GetUserUseCase`) are unaffected.
- Fix: give each route its own use case, even when the underlying
  `call:`/`blob:` binding is identical (e.g. a second `AdminUpdateXUseCase`
  wrapping the same `call: XInfra.update_one`) — cheap in `.tr` source,
  and it keeps the door open for the two routes to diverge later (e.g.
  different `errors {}` mappings) without a rename.

## `grpc_client` Storage Coverage

- Only `find_one` is implemented as a mappable `grpc_client` storage method.
- Mapping any other operation (`save_one`, `save_many`, `find_many`, `list`,
  `update_one`, `delete`, `delete_many`) in `methods { ... }` is accepted by
  the parser but rejected at codegen time.

## gRPC Field Type Coverage

- `rpc`/gRPC message fields only support `String` and `Int` (`i64`).
- `Uuid`, `Float`, `Decimal`, list-shaped (`Vec<...>`) outputs, and nested
  struct fields are a hard **generation-time** error ("gRPC doesn't support
  field type '...' yet — only String and Int fields are supported"). Design
  any gRPC-exposed type with flat scalar fields only.
- gRPC error-to-status mapping is entirely hardcoded (`NotFound` →
  `Status::not_found`, everything else → `Status::internal`) — there is no
  `status {}`-equivalent customization point for `rpc`, unlike HTTP routes.

## HTTP Status Mapping

- `NotFound` and `StorageError` always map to `404`/`500` respectively and
  **cannot be overridden** by a route's `status {}` block, even if you write
  an explicit `NotFound -> 400` entry — the generated code emits the
  hardcoded arms unconditionally.

## Load Balancer Targets

- `load_balancer { targets { ... } }` entries are always static string
  literals. Unlike adapter `on:` and the load balancer's own `on:`, `${VAR}`
  environment interpolation does **not** apply to `targets` — changing the
  backend fleet requires recompiling.

## `Regex<...>` Type Semantics

- `Regex<...>` syntax exists as a declared type form.
- It compiles down to a plain `String` field — the pattern is discarded at
  codegen and no runtime validator is ever generated or enforced.
- The pattern itself is also restricted at the lexer level to a single
  alphanumeric/`_` word — real regex metacharacters break lexing.

## AI Provider Support

- `anthropic` is the only supported provider path in the current
  implementation; any other `--ai-provider` value errors the first time an
  `ai {}` use case needs to call out.
- AI use cases always get a fixed `{NotFound, StorageError(String)}` error
  enum — `errors {}` customization isn't available for `ai` use cases the
  way it is for `call`-based ones.

## Plugin Constraints

- Plugins must match the expected ABI version (currently version `1`) — no
  partial/best-effort compatibility across versions.
- Plugin-generated output file names are restricted to safe, single-segment
  names (no `/`, `\`, or `..`).
- A plugin block is a single opaque top-level construct: it can't introduce
  a new field/expression *type* usable elsewhere in the language, and can't
  make other builtin constructs recognize new syntax.
- There is no plugin discovery/registry — every invocation must pass
  `--plugin=<path>` explicitly; nothing is remembered between runs.

## CLI / Process Behavior

- A missing or unreadable `.tr` file path causes a Rust `panic!` (process
  exit code 101), not the same clean `eprintln!` + `exit(1)` path used for
  every lex/parse/plugin/codegen error.
- A successful compile currently prints the full lexed token stream and the
  full parsed AST to stdout unconditionally — this isn't gated by `--dev` or
  any verbosity flag, and there's no way to suppress it today.
- `./out` (the generation target, currently hardcoded and not
  flag-configurable) is fully deleted and recreated on every run — there is
  no incremental generation and no way to preserve hand-edits.
