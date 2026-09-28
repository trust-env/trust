# CLI Reference

## Invocation

```bash
cargo run -- <file.tr> [file2.tr ...] [flags...]
# or, with a release build:
./target/release/trust <file.tr> [file2.tr ...] [flags...]
```

You can pass more than one `.tr` file in one invocation — each is lexed,
parsed, and compiled **independently** (in the same process, sharing only
the loaded plugin set). There's no cross-file linking between separately
passed files beyond what `import` already does.

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

You only need the `--ai-*` flags if the `.tr` file actually contains an
`ai {}` use case; files without one never touch these settings.

## Output

Generated code is always written to `./out` relative to the current working
directory — this path is currently hardcoded, not configurable via a flag.

> **`./out` is destroyed and rebuilt on every run.** Before generating
> anything, Trust does `remove_dir_all("./out")` followed by
> `create_dir_all("./out")`. Any manual edits you made inside `out/` are
> gone the next time you compile — treat everything under `out/` as fully
> disposable, generated-only output, never a place to hand-edit.

If the `.tr` file declares one or more named `app { ... }` blocks, `./out`
becomes a **Cargo workspace** — even with just one `app` — with one member
crate per app (named after that app's snake_cased name) and a root
`Cargo.toml` (`[workspace] resolver = "2"`, `members = [...]`). A `.tr` file
with **zero** `app` blocks falls back to a single flat crate directly under
`./out` (mainly relevant for isolated test fixtures — real projects should
always declare an `app`).

## stdout / stderr behavior

A successful run currently prints, to **stdout**, unconditionally (not
gated behind `--dev` or any verbosity flag):

1. The **entire lexed token stream** for every input file (`Tokens: {:#?}`).
2. The **entire parsed AST** for every input file (`Parsed ASTs: {:?}`).

This is leftover debug output from early development, not a documented
feature — expect a large, noisy dump on stdout for anything beyond a
one-line fixture. There is currently no `--quiet`, no `-v`/verbosity flag,
and no `RUST_LOG` support to suppress or adjust it.

Errors are printed to **stderr**, with a consistent `Error occurred while
<stage>: <message>` prefix, followed by a clean `exit(1)` — except for one
case:

| Failure | Where reported | Exit behavior |
|---|---|---|
| Lex error | stderr, `"Error occurred while lexing: {e}"` | clean `exit(1)` |
| Parse error | stderr, `"Error occurred while parsing: {e}"` | clean `exit(1)` |
| Plugin load error (bad path, ABI mismatch, keyword collision) | stderr, `"Error occurred while loading plugin '{path}': {e}"` | clean `exit(1)`, stops at the **first** failing plugin |
| Plugin template registration error | stderr, `"Error occurred while registering plugin templates: {e}"` | clean `exit(1)` |
| Codegen error | stderr, `"Error occurred while generating code: {e}"` | clean `exit(1)` |
| **A `.tr` file path that doesn't exist or can't be read** | a Rust `panic!` message (`"Failed to read file"`) | **process panic, exit code 101** — not the same clean path as every other error. If you see a panic instead of a tidy error, double-check your file paths first. |

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

- One line per `psql` infra: `<INFRA_NAME>_DB_URL=postgres://postgres:postgres@localhost:5432/<entity>` (env var name is the infra's name, upper-snake-cased, entity name lowercased for the DB name). `grpc_client` infras are skipped — their target address is a compile-time literal, not env-driven.
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
