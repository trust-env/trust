---
name: trust-dsl-build
description: "Use when: writing or editing a `.tr` file to build a Trust-based service, running the `trust` compiler to generate a project, or diagnosing why a `.tr` file won't parse/generate/build. Trigger phrases: add a route, add a use_case, wire up psql/grpc/auth/load_balancer/ai, compile this .tr file, why won't my .tr file build, new Trust app, extend user.tr. This is the authoring/consumer-facing skill (writing `.tr` source and driving the CLI) — for the shape of the *generated Rust output* itself, use the sibling `trust-generated-architecture` skill; for compiler internals (lexer/parser/generator source), read `src/` directly."
---

# Building things with the Trust DSL

Trust compiles a declarative `.tr` file into a runnable Rust Cargo
workspace. This skill is for the "I want to write/extend a `.tr` file and
get a working service out of it" workflow — not for hacking on the compiler
itself.

**Full reference docs live in `doc/` in this repo** — this skill is a
condensed cheat sheet plus the workflow and gotchas that aren't obvious from
skimming those files. Go to `doc/` for anything this file doesn't answer:

| Need | File |
|---|---|
| What does keyword X do / exact syntax | `doc/keywords.md` |
| Feature catalog (what's possible at all) | `doc/features.md` |
| Exact `.tr` → Rust conversion rules (types, names, error enums, etc.) | `doc/conversions.md` |
| CLI flags, exit codes, dev mode | `doc/cli.md` |
| Copy-paste patterns | `doc/recipes.md` |
| Known gaps/limitations before you assume something works | `doc/limitations.md` |

## The five building blocks

Every `.tr` file is some combination of these, in **any order** (forward
references are fine — the compiler resolves names in a pre-pass):

```tr
type Task {                       // 1. data shape
    id: Int,
    title: String
}

infra TaskInfra {                 // 2. storage backend
    storage {
        psql {
            entity: Task
        }
    }
}

use_case CreateTaskUseCase {      // 3. business logic
    call: TaskInfra.save_one
}

adapter HttpApi {                 // 4. how it's exposed
    http {
        on: "0.0.0.0:8080"
        routes {
            POST "/tasks" {
                use_case: CreateTaskUseCase
            }
        }
    }
}

app Main: run {                   // 5. what runs together as one binary
    HttpApi
}
```

Nothing compiles to a runnable binary without an `app` block — a file with
`type`/`infra`/`use_case`/`adapter` but no `app` still parses and generates,
but falls back to one flat, non-workspace crate. Always add `app` for real
work.

## Workflow

1. Edit the `.tr` file.
2. Compile it: `<trust-binary> your_file.tr` (see [Running the
   compiler](#running-the-compiler) below for how to find/build that
   binary from a project that isn't `compiler/` itself, e.g. `user/`).
3. **Check for `Error occurred while ...` on stderr before anything else** —
   a clean run still dumps the full token stream and full AST to stdout
   (known noise, not a bug you introduced — see `doc/cli.md`). Don't mistake
   that dump for an error.
4. If generation succeeded, verify it actually compiles as Rust:
   `cd out/<app_name> && cargo build` (or `cd out && cargo build` for a
   workspace root, which builds every app at once). Trust generating
   *something* doesn't guarantee that something is valid — always build.
5. Iterate. **`./out` is fully deleted and rebuilt every single compile** —
   never hand-edit anything under `out/`, it will not survive step 2 next
   time you run it.

### Running the compiler

From inside `compiler/`: `cargo run -- <file.tr> [flags]`.

From a sibling consumer project (e.g. `user/`, which is its own repo with
its own `user.tr` and generates into its own `./out`), invoke the built
binary directly rather than `cargo run`ning the compiler crate from the
wrong directory:

```bash
# one-time build, from compiler/
cd ../compiler && cargo build       # produces target/debug/trust
# then, from the consumer project directory:
../compiler/target/debug/trust user.tr --dev
```

Useful flags (full table in `doc/cli.md`):
- `--dev` — adds a live `/app` HTML docs route per HTTP adapter, and
  scaffolds a starter `.env` (DB URLs, `${VAR}` defaults, `JWT_SECRET` if
  any route needs auth). Always use this while iterating locally.
- `--plugin=/abs/path/to/lib*.so` — load a compiler plugin (repeatable).
- `--ai-api-key=...` (or `ANTHROPIC_API_KEY` env var) — required only if
  the file has an `ai { ... }` use case.

## Fast keyword reference

(Full syntax and every edge case: `doc/keywords.md`.)

- `type Name { field: T, ... export Name }` — struct; `type Alias = T` — alias.
- `infra Name { storage { psql { entity: T } } }` — Postgres CRUD backend.
  Shorthand: `infra: storage psql Name { entity: T }`.
- `infra Name { storage { grpc_client { service on entity methods { verb: Rpc } } } }`
  — cross-binary storage. **Only `find_one` actually works today.**
- `infra Name { storage { s3 { bucket: "..." entity: T } } }` — MinIO/S3
  blob storage. `entity:` is metadata-only; endpoint/access key/secret key
  are **never** in `.tr` source — always three env vars derived from the
  infra name (`<INFRA>_S3_ENDPOINT`/`_S3_ACCESS_KEY`/`_S3_SECRET_KEY`).
- `use_case Name { input output call: Infra.method ... }` — deterministic
  logic. Multiple `call:` lines are allowed; the **last** one shapes the
  output, earlier ones run first as shared side effects.
- `use_case Name { ai { infra prompt } }` — LLM-authored body, generated
  **once at compile time** (not a runtime call). Mutually exclusive with
  `call`/`blob`.
- `use_case Name { blob: S3Infra.presigned_put_url }` — binds straight to
  one `s3 {}` operation (`presigned_put_url`/`presigned_get_url`/`delete`).
  Fixed `{ key: String }` in / `{ url: String }` (or `{ key: String }` for
  `delete`) out — don't declare your own `input`/`output`. Mutually
  exclusive with `call`/`ai`.
- `use_case Name { call: Infra.list filters { field: input.field ... } }`
  — scopes a `list` call's results by one or more **equality-only**
  predicates. Only valid when the primary call is `list`. Leave `input`
  undeclared and the filtered field is auto-added to the guessed
  `{ limit, offset }` input — a route's `:field` path param then reaches
  it for free via the existing name-matching, so `GET
  "/spots/:spot_id/ratings"` on a `filters { spot_id: input.spot_id }`
  use case just works with zero extra wiring.
- `adapter Name { http { on routes { METHOD "/path" { input output use_case status auth } } } }`
- `adapter Name { grpc { on services { rpc Name { use_case } } } }`
- `app Name: run { AdapterA AdapterB load_balancer L { targets { "..." } on: "..." } }`

## Traps that will bite you (condensed from `doc/limitations.md`)

- **No comments.** `//`, `#`, `/* */` are all hard lexer errors.
- **`import X from "git@..."` parses but fails at codegen.** Only local
  path imports (`"./other.tr"`) actually work, despite the README's own
  example showing git.
- **A route/`rpc`'s `input:`/`output:` don't define anything** — they're
  only an optional cross-check against the use_case's own signature. The
  use_case's `input`/`output` (declared or inferred) is what's authoritative.
- **`load_balancer` `targets` are always static strings** — `${VAR}`
  interpolation only works on `on:` (adapters and the load balancer's own
  listen address), never on `targets` entries.
- **`NotFound`/`StorageError` HTTP statuses are hardcoded** (404/500) and
  cannot be overridden by a route's `status {}` block.
- **gRPC message fields only support `String` and `Int`** — `Uuid`, `Float`,
  `Decimal`, list outputs, and nested structs are a generation-time error.
- **`Regex<pattern>` discards the pattern** — it's just `String` with no
  validation generated, ever.
- **`Int` is always `i64`**, never `i32`. `Float` is `f64`. `Decimal` is
  `rust_decimal::Decimal`, serialized as a JSON **string** (not a number) —
  use it instead of `Float` for anything money-shaped.
- **`s3 {}` storage doesn't implement `Storage<T, E>`** — a `use_case`
  binds to it with `blob:`, never `call:`. A `blob {}` use case's
  input/output shape is fixed and non-overridable — declaring your own
  `input`/`output` is a hard error, not a narrowing.
- **`filters {}` is equality-only and AND-only** — no `>`/`<`/`LIKE`, no
  OR between entries, and it only works on a `list`-bound primary call
  (not `find_many`).
- A missing/unreadable `.tr` file path panics (exit 101) instead of
  printing a clean error — double-check paths first if you see a panic.

## When something won't parse

Top-level constructs are tried in a fixed order
(`adapter → import → type → infra → use_case → plugin → app`) and a total
parse failure always reports the error from the **last** alternative
(`app`) — not necessarily the construct you actually got wrong. If the
error message looks unrelated to what you wrote, check your syntax against
the concrete examples in `doc/keywords.md` and `doc/recipes.md` rather than
trusting the message's literal wording.

## Reference examples in this repo

- `examples/*.tr` — small syntax-focused samples, one feature each.
- `scenarios/0N-*/app.tr` + `scenario.md` — larger, **live-executed** proofs
  (real Postgres, real spawned processes, real HTTP requests) for CRUD,
  gRPC+load-balancer, AI+plugins, multi-app workspaces, and cross-binary
  gRPC. Copy from these for anything beyond a single CRUD endpoint.
- `doc/recipes.md` — the same patterns, isolated and annotated.
