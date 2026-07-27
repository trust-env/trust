# Scenarios

Feature-coverage smoke tests for the Trust compiler: each scenario is a
self-contained `.tr` fixture plus a bash script that generates it, checks
the output, and proves the generated Rust project actually compiles —
independent of the `cargo test` suite, runnable on its own.

| # | Scenario | Proves |
|---|----------|--------|
| [01](01-crud-http-psql/scenario.md) | CRUD HTTP + PSQL storage | The core everyday pipeline: `type` → `psql` storage → `use_case` → `http` adapter. |
| [02](02-grpc-load-balancer/scenario.md) | gRPC + load balancer across separate processes | `grpc {}` coexisting with a `load_balancer {}` that proxies to 3 real, independently-deployed OS processes (own DB each) — not in-process fan-out. |
| [03](03-ai-and-plugin-extensibility/scenario.md) | AI use case + plugin | `ai {}` (mocked LLM) and a dynamically loaded `--plugin` both working, individually and together. |
| [04](04-multi-app-workspace/scenario.md) | Multiple named apps | Two named `app` blocks compiling into two independent, self-contained binaries/crates in one generated Cargo workspace. |
| [05](05-cross-binary-grpc/scenario.md) | Cross-binary gRPC call | A use case in one app calling, over a real gRPC connection, an adapter running in a *different* compiled binary — actually starts both processes and makes a real HTTP request, not just a compile check. |

Run any one directly:

```bash
./scenarios/01-crud-http-psql/run.sh
./scenarios/02-grpc-load-balancer/run.sh
./scenarios/03-ai-and-plugin-extensibility/run.sh
./scenarios/04-multi-app-workspace/run.sh
./scenarios/05-cross-binary-grpc/run.sh
```

Or all of them:

```bash
for s in scenarios/*/run.sh; do "$s"; done
```

Each script builds the `trust` CLI, generates its `.tr` fixture into a
throwaway temp directory (cleaned up on exit), checks specific generated
files/content, then runs a real offline `cargo build` against the generated
project — every compile now produces a Cargo workspace with one member crate
per named `app` block (scenario 04 is the one that actually declares more
than one). Scenarios 02 and 05 go further and actually run the compiled
binaries as real processes (both need Docker, for a throwaway Postgres —
scenarios 01/03/04 need neither Docker nor a live process). `scenarios/lib.sh`
holds the logic shared across all of them (workdir setup, patching every
generated member crate's private git dependencies to this repo's local
`mod/*` checkouts, the mock Anthropic server used by scenario 3, the
throwaway Postgres container + port-wait polling used by scenarios 2 and 5,
...) — read it if you're adding a new scenario.
