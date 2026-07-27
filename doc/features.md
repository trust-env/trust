# Features

## 1. Type System

What it does:
- Defines domain entities, request, and response shapes.
- Supports built-in scalar types (`String`, `Uuid`, `Int`, `Regex<pattern>`) and user-defined types.

How to use:
- Create struct types with fields.
- Use aliases for reusable semantic names (`type UserId = Int`).
- Use `export` on types you plan to import from other `.tr` files.

Conversions: see [conversions.md §1](conversions.md#1-scalar-type-mapping) —
notably, `Int` becomes `i64` (not `i32`), and `Regex<pattern>` becomes a
plain `String` with the pattern discarded (no runtime validation is
generated).

## 2. Import and Reuse

What it does:
- Reuses exported Trust types across modules.

How to use:
- Use `import X from "./module.tr"` for one item.
- Use `import { A, B } from "./module.tr"` for many.

Notes:
- Local module path imports are fully supported.
- Git import syntax (`import X from "git@..."`) is parsed, but code
  generation currently rejects it with a clear error — despite appearing in
  the top-level README's own DSL example. Only local paths work end to end
  today.

## 3. Storage Infrastructure

### PostgreSQL (`psql`)
What it does:
- Generates a full CRUD/list storage implementation (`save_one`, `save_many`,
  `find_one`, `find_many`, `list`, `update_one`, `delete`, `delete_many`).

How to use:
- Declare `infra` with `storage { psql { entity: YourEntity } }`, or the
  one-line shorthand `infra: storage psql Name { entity: YourEntity }`.

### gRPC Client Storage (`grpc_client`)
What it does:
- Lets one app call another app's gRPC adapter through the same storage
  trait mapping `psql` uses — so downstream `use_case`/`call:` syntax is
  identical regardless of backend. This is the mechanism behind cross-binary
  calls in a multi-app workspace.

How to use:
- In `methods`, map storage operations to RPC names — but **only `find_one`
  actually has codegen today**; other mapped methods parse but error at
  generation time.
- Set the static target address with `on`.

## 4. Use Cases

What it does:
- Defines application behavior and error mapping, either deterministically
  (`call`) or via AI-authored code (`ai`).

How to use:
- Deterministic mode: one or more `call` entries. Multiple `call`s run in
  order, sharing one input; the **last** one shapes the output (see
  [conversions.md §5](conversions.md#5-use_case-multi-call-semantics)).
- AI mode: one `ai` block with `prompt`, optionally with `infra:` for
  storage access. Mutually exclusive with `call`.
- Optionally provide explicit `input` and `output` types.

## 5. HTTP Adapter

What it does:
- Exposes use cases as REST endpoints via axum.

How to use:
- Define routes per method and path (path params like `:id` become axum's
  `{id}` syntax automatically).
- Attach `input` (bare type or `{ body: Type }` wrapped), `output`, `use_case`.
- Add route-level `status` mappings and optional `auth`.

Supported methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`.

`NotFound`/`StorageError` always map to `404`/`500` and cannot be
overridden; other error variants resolve via an exact `status {}` entry,
then the route's `_ ->` wildcard, then a `500` fallback.

## 6. gRPC Adapter

What it does:
- Exposes use cases as gRPC RPC methods, generates `.proto` + tonic server
  wiring, and vendors `protoc` itself (no system `protoc` install needed).

How to use:
- Define `grpc { services { rpc ... } on: "..." }` under an adapter.

Limitations (real, not currently in `limitations.md` at equal visibility to
the `grpc_client` one):
- Message fields only support `String` and `Int` — `Uuid`, list-shaped
  outputs, and nested struct fields are a generation-time error.
- Error mapping is entirely hardcoded (`NotFound` → `Status::not_found`,
  everything else → `Status::internal`); there is no `status {}` equivalent
  for `rpc`.

## 7. App Composition and Multi-App Workspaces

What it does:
- Builds one or many binaries from one Trust source file.

How to use:
- Declare multiple `app` blocks when you need separate runtimes.
- Put different adapters/roles in each app.

Notes:
- Any file with **at least one** `app` block compiles to a Cargo workspace
  (`[workspace] members = [...]`) — even with just one `app`. Zero `app`
  blocks falls back to a single flat crate.
- Declaration order in the source file never affects what's generated —
  forward references (an adapter mentioning a use case declared later) work
  by design and are regression-tested for byte-identical output.

## 8. Load Balancer Generation

What it does:
- Generates a round-robin reverse proxy over independently-deployed,
  already-running backend processes — it never constructs an adapter itself.

How to use:
- Put `load_balancer` inside an `app ... run` block.
- Configure `targets` (backend addresses) and `on` (its own bind address).

Notes:
- `targets` entries are always static string literals — `${VAR}`
  interpolation only applies to the load balancer's own `on:`, not to
  `targets`. Changing the backend fleet requires recompiling.

## 9. Auth (Route-Level Authorization)

What it does:
- Gates an HTTP route behind a JWT-authenticated permission-row lookup.

How to use:
- Add `auth { infra: PermissionInfra target: "..." action: "..." }` inside a
  route.

Mechanics:
- The handler extracts a `UserContext` from the JWT (via the external `auth`
  crate), then queries `infra:`'s storage for a row matching the user, the
  given `target`, and the given `action`.
- No matching row → `403 Forbidden`. A storage failure during the lookup →
  `500`.
- Adds the `auth` crate as a dependency; in `--dev` mode, scaffolds
  `JWT_SECRET=changeme` in `.env` whenever any route needs it.

## 10. AI Use Case Generation

What it does:
- Generates a use case's function body through a real, live LLM call made
  **once, at compile time** — not at runtime. The generated project itself
  gets no AI-related dependency and never calls out to any provider.

How to use:
- Add `ai { prompt: '...' }` (optionally with `infra:`) in a use case.
- Run the compiler with AI flags or the `ANTHROPIC_API_KEY` env var.

CLI-related flags (full detail in [cli.md](cli.md)):
- `--ai-provider=anthropic` (only supported value today)
- `--ai-api-key=...` (flag takes precedence over the `ANTHROPIC_API_KEY` env var)
- `--ai-model=...` (default `claude-sonnet-4-5`)
- `--ai-base-url=...` (default `https://api.anthropic.com`)

## 11. Plugin Extensibility

What it does:
- Dynamically loads compiler plugins (`.so`/`cdylib`) over a stable C-ABI +
  JSON boundary. A plugin registers exactly one new top-level keyword and
  ships its own Tera codegen templates; the compiler's parser only
  brace-matches the block and hands the plugin the raw source text — the
  plugin does its own parsing and its own codegen decisions.

How to use:
- Pass one or more `--plugin=/absolute/path/to/libplugin.so` flags.
- Use the plugin's registered keyword block in your `.tr` source.

Scope note: a plugin can add a new top-level construct, but it can't
currently introduce a new field/expression *type* usable elsewhere in the
language, and there's no plugin registry — every invocation must pass
`--plugin=` explicitly.

## 12. Dev Mode Artifacts

What it does:
- Adds a live `GET /app` HTML docs route to every HTTP adapter (method,
  path, path-params, input/output field tables, error/status table — not a
  static file, it's baked into the generated handler code) and scaffolds a
  starter `.env` per app (DB URLs, sniffed `${VAR}` defaults, `JWT_SECRET`
  if needed).

How to use:
- Run with `--dev`. Full defaulting rules: [cli.md](cli.md#dev-mode---dev).

## 13. Scenarios and E2E Workflows

What it does:
- Demonstrates complete, live-executed workflows (not just compile checks):
  - CRUD HTTP + PSQL (compile + build)
  - gRPC + load balancer (spawns 3 real OS processes, each its own Postgres,
    confirms round-robin cycling)
  - AI + plugin (mocked LLM HTTP server + real dlopen'd plugin)
  - multi-app workspace (two independent binaries from one `.tr` file)
  - cross-binary gRPC (two compiled binaries, real Postgres, a live HTTP
    request that round-trips through gRPC end to end)

How to use:
- Run scenario scripts under `scenarios/*/run.sh`.

## Practical gotchas worth knowing before you start

- `./out` is fully wiped (`remove_dir_all` + `create_dir_all`) on **every**
  compile — never hand-edit generated code and expect it to survive.
- A successful compile currently prints the full token stream and full AST
  to stdout unconditionally — expect noisy output, this isn't a `--dev`-only
  thing and isn't suppressible today.
- A missing/unreadable `.tr` file path causes a Rust panic (exit code 101),
  not the clean `exit(1)` every other error path uses.
