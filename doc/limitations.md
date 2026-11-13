# Current Limitations

## External Rust Type Reuse

- Directly importing an existing Rust `.rs` type/class into Trust DSL is not supported.
- Trust imports operate on exported Trust types from `.tr` modules.

## Git Imports

- Git-style module import syntax is recognized by the parser.
- Code generation currently rejects git imports ("Git imports are not
  supported yet"). Only local path imports (and `include`) work.

## `filters` Coverage

- **AND-only, no OR.** Every `filters {}` line applies together — there's no
  way to express alternatives (`a = X OR b = Y`).
- **No `IS NULL`.** A filter can't select rows where a nullable column is
  empty.
- `in` lists travel as one comma-separated string: a value containing a
  comma can't be matched with `in`.
- Only on a `list` primary call (`find_many` takes ids, not filters), and
  only on scalar columns (not JSONB lists/structs).
- `today()` without a zone is the date in the `config { tz }` zone, or in
  UTC without one.

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

## `owner`, `code` and `RawRequest`

- `owner:` only works on `call`-based use cases, with an `Int` owner field,
  exposed over HTTP routes with `auth` — gRPC can't authenticate callers,
  so an `owner` use case can't be an `rpc`.
- `code {}` bodies are plain text pasted into `execute`: Trust doesn't
  check them — mistakes surface when the generated crate is compiled, with
  line numbers pointing into `applications/<use_case>.rs`, not the source
  file.
- `code {}` error variants always carry a `String` payload.
- `RawRequest` is HTTP-only, can't be combined with path parameters, and
  reads the body as UTF-8 text (no binary uploads).
- `http_client`'s configured headers are static per process. OAuth2 with a
  refresh token is built in (`oauth2 {}`); other per-request headers go
  through `request(...)` + `send_raw(...)`.

## `logic`

- Only `psql` and `http_client` infras and other use cases are reachable;
  `s3`, `grpc_client` and notification channels need `code {}`.
- Types imported from another module (`import`) aren't visible to a
  script; declare them in the project.
- Tagged enums (variants with fields) can be passed around but not built
  or matched; struct values can't be compared with `==`.
- Queries can't match `none` and have no `or`; `TaskInfra.query(...)`
  returns at most 1000 rows unless `limit:` says otherwise.
- `Int` arithmetic other than `/` and `%` wraps on overflow in release
  builds (and panics in debug builds), like Rust.
- No user-defined functions or shared library between scripts, and no
  error recovery (a failing call ends the script): use `code {}` with
  `include:` for those.

## Migrations

- Added fields become new (nullable) columns; `renamed {}` and `dropped`
  handle renames and removals. **Retyped** fields are not migrated: change
  the column type by hand (`ALTER TABLE ... ALTER COLUMN ... TYPE`).
- Foreign keys are only generated toward another `psql` infra's `id`, and
  need both tables in the same database.
- Indexes are created with a plain `CREATE INDEX IF NOT EXISTS` at startup,
  which blocks writes to that table while the index builds — on a large
  existing table, create it `CONCURRENTLY` by hand before deploying.
- `seed:` files re-run on every startup; they must be idempotent.
- Infras pointing to the same database URL share one connection pool; a
  `database {}` block sets its size. Infras on different URLs can't share a
  transaction or a foreign key.

## HTTP Status Mapping

- `NotFound` and `StorageError` always map to `404`/`500` respectively and
  **cannot be overridden** by a route's `status {}` block.
- A database constraint violation (unique key, foreign key) is a
  `StorageError` (`500`) unless the call maps `DatabaseError` in its
  `errors {}` block.

## Load Balancer Targets

- `load_balancer { targets { ... } }` entries are always static string
  literals. Unlike adapter `on:` and the load balancer's own `on:`, `${VAR}`
  environment interpolation does **not** apply to `targets` — changing the
  backend fleet requires recompiling.

## `Regex<...>` Type Semantics

- Validation happens when JSON / query strings are decoded. Values built in
  `code {}` bodies or read from the database are not re-checked.
- `List<Regex<...>>` items and `Option<Regex<...>>` fields of a `patch_one`
  input are not validated.

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

- The output directory is fully deleted and recreated on every run — there
  is no incremental generation and no way to preserve hand-edits.

## Schedules

- Scheduled runs build their input with `Default::default()`; a use case
  needing real arguments must read them itself (from storage, settings).
- A schedule runs inside one process: with several replicas of the same app,
  every replica runs every job.
- `tz` defaults to `config { tz }`, else UTC.

## MCP Adapter

- Tools and resources (no prompts, resource templates or server-initiated
  notifications); `GET /mcp` (a server-to-client stream) is not offered.
- `oauth {}` is single-user: one password signs in as one user. Pending
  authorization codes live in memory (a restart drops sign-ins in progress).
- `<binary> mcp` serves the first `mcp {}` adapter of the app over stdio.
- The HTTP transport doesn't check the `Origin` header: bind it to
  `127.0.0.1` or put it behind auth.

## `--emit-ts` / `--emit-openapi`

- Only HTTP adapters are described (not gRPC or MCP).
- Schemas are inlined (no shared `components/schemas`).

## `journal` (undo)

- Only `psql` rows written through the use case's storages are recorded:
  calls to an `http_client` (an email sent, an event pushed to Google) are
  not undone.
- Undo restores rows by `id` (a deleted row comes back with the same id);
  every table involved must have an `id: Int` and be on the journal's
  database.
- `conflict: refuse` compares whole rows: a change to any column of a row
  since the entry (other than reaching the undo's own target state) is a
  conflict.

## `resource`

- A `singleton` resource only has `get` and `upsert`.
- A resource covers one infra; joins and computed fields need a `code {}`
  use case.

## `mirror`

- The source is an `ics_feed` infra or a use case whose output has an
  `items` list; other shapes go through a small `code {}` use case.
- Items are upserted one by one, and `written` counts every item of the
  source, changed or not.
- `scope {}` takes equalities and `> window.from` / `< window.to`
  comparisons only.
- `skip {}` rules are OR-ed (any matching line skips) and read direct
  fields of the item (`item.a.b` through a struct, not through an
  `Option`).
- Literal values of `map {}` are converted when the mirror runs: one that
  doesn't fit its field is reported in `error`, not at compile time.
- Writing back to the source (pushing local rows to an API) is not
  generated: keep it in `code {}`.

## `ics_feed`

- `VTIMEZONE` definitions are not read: a TZID must be an IANA name or a
  Windows name from CLDR's table; anything else is read in
  `timezone_fallback` (with a warning).
- A recurring event yields at most 5000 occurrences per read.
- All-day events are skipped unless `all_day: include`.
- Attendees, organizer, alarms and attachments are not exposed.

## `channel`, `notifications` and `bot`

- Channels: Telegram, webhooks (`discord`, `slack` or plain `json` bodies)
  and `log`. No e-mail or SMS channel yet: use a `webhook` towards a
  service that sends them.
- Routing is fixed in the `.tr` file (no per-user preferences).
- Bots are Telegram only, read text messages only (no photos, voice or
  buttons), and run every message as the single `user:` they declare.
- Bot commands and the default receive `{ text }`; the reply template reads
  the use case output only.

## `map`

- Values are source fields (possibly nested) or literals: no expressions,
  no parameters. For a conversion needing either, write a Rust function in a
  `code { include }` file.

## `config { tz locale }`

- One time zone and one locale per project.
- Locales: `fr` and `en`.
