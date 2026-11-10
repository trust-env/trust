# CLI Reference

## Invocation

```bash
cargo run -- <file.tr> [file2.tr ...] [flags...]
# or, with a release build:
./target/release/trust <file.tr> [file2.tr ...] [flags...]
```

Several `.tr` files passed in one invocation form **one project**, exactly
as if an entry file `include`d them all. A project usually has one entry
file that `include`s its modules.

Only the `--key=value` form is supported for value-carrying flags — there is
no `--key value` (space-separated) form. Anything starting with `--` that
isn't a recognized boolean/value flag is stored but currently unused; any
argument that doesn't start with `--` is treated as a `.tr` file path.

## Flags

| Flag | Kind | Default | Notes |
|---|---|---|---|
| `--dev` | boolean (bare) | off | Must be written exactly `--dev` — `--dev=true` is not recognized as the boolean form (it *would* be parsed as the string `"true"` under the key `dev`, but nothing reads that key; only the literal `--dev` token is checked). See [Dev mode](#dev-mode---dev) below. |
| `--plugin=<path>` | repeatable value | none | Loads a `cdylib` plugin from `<path>` via `libloading`. Pass multiple times to load multiple plugins. See the plugin docs for the `.so` contract. |
| `--ai-provider=<name>` | value | `anthropic` | Only `anthropic` is currently implemented; anything else is a hard error the first time an `ai {}` use case actually needs to call out. |
| `--ai-api-key=<key>` | value | none | Falls back to the `ANTHROPIC_API_KEY` environment variable if omitted. **The flag wins if both are set.** |
| `--ai-model=<name>` | value | `claude-sonnet-4-5` | — |
| `--ai-base-url=<url>` | value | `https://api.anthropic.com` | Mainly useful for pointing at a local mock server in tests. |

| `--out=<dir>` | value | `./out` | Where the project is generated. The directory is wiped first, so Trust refuses one that contains the working directory, or a non-empty one without a root `Cargo.toml`. |
| `--vendor=<dir>` | value | none | Copies every private git dependency the output needs (`common`, `auth`, `user`, ... and theirs) from `<dir>/<crate>` — or, when missing there, from Cargo's git cache (`~/.cargo/git/checkouts`) — into `<out>/vendor/` and rewrites the manifests to `path` dependencies. The output then builds with no SSH access (Docker, CI). |
| `--emit-ts=<file>` | value | none | Writes a typed TypeScript client: one declaration per type the HTTP routes use and one `<Adapter>Client` class per HTTP adapter. |
| `--emit-openapi=<file>` | value | none | Writes an OpenAPI 3.1 document of every HTTP route. |
| `--verbose` | boolean | off | Prints the token stream and the parsed AST to stderr. |

You only need the `--ai-*` flags if the `.tr` file actually contains an
`ai {}` use case; files without one never touch these settings.

## Output

Generated code is written to `./out` (or `--out=<dir>`).

> **The output directory is destroyed and rebuilt on every run.** Never
> hand-edit generated code: it is gone the next time you compile.

If the `.tr` file declares one or more named `app { ... }` blocks, the
output is a **Cargo workspace** — even with just one `app` — with one member
crate per app (named after that app's snake_cased name) and a root
`Cargo.toml`. A `.tr` file with **zero** `app` blocks falls back to a single
flat crate.

## stdout / stderr behavior

Lex and parse errors point at the source: `modules/inbox/inbox.tr:12:18:
message`, followed by the offending line and a caret.

A successful run prints one line per output (`Generated ./out`, `Vendored
...`, `Wrote TypeScript client to ...`). Errors go to stderr as `Error
occurred while <stage>: <message>`, followed by `exit(1)` — lexing,
parsing, reading an input file, loading plugins, generating, vendoring and
emitting all follow this path.

## Generated binary commands

Besides running its servers, a generated binary accepts:

| Command | When | What |
|---|---|---|
| `<binary> token <user_id> [days]` | the app has an `auth` route (JWT) | Prints a JWT for that user, valid `days` days (365), signed with `JWT_SECRET` — for agents, scripts, the MCP server. |
| `<binary> mcp` | the app runs an `mcp {}` adapter | Serves the first MCP adapter over stdin/stdout instead of starting the network servers. Acts as the adapter's secret user, or `MCP_USER_ID`. |

Logs go to stderr: `RUST_LOG` filters them (default `info`), `LOG_FORMAT=json`
prints one JSON object per line.

## Dev mode (`--dev`)

Passing `--dev` changes generated output in two ways, scoped **per app**
(i.e. in a multi-app workspace, each app gets its own independent dev
artifacts):

### 1. A live docs endpoint

Every HTTP adapter gets an extra route, `GET /app`, serving a self-contained
HTML page (nothing written to disk separately — the HTML is embedded as a
`&'static str` inside the generated Rust source itself). The page has one
section per route: HTTP method + path (with path params called out), an
input-fields table, an output-fields table, and an error/status table
listing `NotFound`/`StorageError` (always present) plus every custom error
target, each with its resolved HTTP status code. All user-supplied strings
are HTML-escaped.

### 2. A scaffolded `.env` file

Written to **that app's own crate root** (`out/<app>/.env` in a workspace —
there is no single shared workspace-root `.env`):

- One line per `psql` infra: `<INFRA_NAME>_DB_URL=postgres://postgres:postgres@localhost:5432/<entity>` (env var name is the infra's name, upper-snake-cased, entity name lowercased for the DB name), followed by `<INFRA_NAME>_DB_POOL_SIZE=16` — the maximum number of Postgres connections that infra's pool keeps open (optional at runtime; 16 when unset). `grpc_client` infras are skipped — their target address is a compile-time literal, not env-driven.
- One line per distinct `${VAR}` referenced by an `http_client` infra's `base_url` or header values, defaulting to `changeme`.
- One line per distinct `${VAR}` actually referenced in any adapter's `on:`/`cors:` or load-balancer's own `on:` (never `targets`, see [conversions.md](conversions.md#7-address-string--env-var-interpolation)):
  - Names containing `PORT` get a shared counter for that app, starting at **3000** and incrementing by 1 for each such variable encountered.
  - Names containing `CORS` or `ORIGIN` default to `"http://localhost:5173"` (Vite's dev server).
  - Names containing `HOST` default to `"0.0.0.0"`.
  - Anything else defaults to the placeholder `"changeme"`.
- `JWT_SECRET=changeme`, appended only if the app has at least one HTTP route with `auth` (either `auth {}` or `auth: required`). The `auth` crate's `UserContext` extractor reads this variable at runtime to validate JWTs.

`--dev` is meant for local iteration — none of this is meant to represent
production secrets; you're expected to replace the scaffolded `.env` values
before deploying.

## Examples

```bash
# Plain compile
cargo run -- app.tr

# Compile with dev docs + scaffolded .env
cargo run -- app.tr --dev

# Compile a file with an ai{} use case
cargo run -- app.tr --ai-provider=anthropic --ai-api-key=sk-ant-...
# or, relying on the environment instead of a flag:
ANTHROPIC_API_KEY=sk-ant-... cargo run -- app.tr

# Compile with one or more plugins loaded
cargo run -- app.tr --plugin=/abs/path/libnote_plugin.so

# Multiple files, multiple plugins, dev mode, all together
cargo run -- app.tr other.tr --dev --plugin=/abs/path/a.so --plugin=/abs/path/b.so
```
