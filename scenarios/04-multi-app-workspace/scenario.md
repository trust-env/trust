# Scenario 4 — multiple named apps, one workspace

## Goal

Prove that Trust can compile **more than one named `app` block from a single
`.tr` file into independent binaries/crates**, each containing only the
adapters/use_cases/infra/types it actually needs — the structural
prerequisite for one binary to eventually run as an HTTP-facing service while
others run as separately deployable gRPC services.

This scenario is deliberately scoped to the *structural* claim only —
partitioning one file into several self-contained, independently-compiling
crates under a generated Cargo workspace — not whether those crates can talk
to each other at runtime. For the latter (a `grpc_client` infra in one app
making a real gRPC call to another app's adapter), see
[scenario 05](../05-cross-binary-grpc/scenario.md).

## What `app.tr` declares

- `Task` / `CreateTaskRequest` — plain data types.
- `TaskInfra` — `psql` storage for `Task`, shared by both apps below.
- `GetTaskUseCase` — shared by both apps' adapters.
- `CreateTaskUseCase` — used only by `HttpApiAdapter`.
- `HttpApiAdapter` — an HTTP adapter (`POST /tasks`, `GET /tasks/:id`).
- `GrpcWorkerAdapter` — a gRPC adapter (`GetTask` rpc), calling the *same*
  `GetTaskUseCase` as the HTTP adapter.
- `app HttpApi: run { HttpApiAdapter }` and
  `app GrpcWorker: run { GrpcWorkerAdapter }` — two named apps, each running
  just its own adapter.

## How to run it

```bash
./scenarios/04-multi-app-workspace/run.sh
```

The script:

1. Builds the `trust` CLI and runs it against `app.tr` in a scratch temp
   directory.
2. Asserts `out/Cargo.toml` is a workspace manifest listing both
   `http_api` and `grpc_worker` as members, and that each member has its own
   `Cargo.toml` with its own `[package]` name.
3. Asserts the shared `Task` type, `TaskInfra` storage, and `GetTaskUseCase`
   were generated into **both** `out/http_api/` and `out/grpc_worker/`
   (each crate is fully self-contained — duplication, not a shared crate).
4. Asserts each crate contains *only* what its own app needs: `http_api` has
   its HTTP adapter and `CreateTaskUseCase`, but not `grpc_worker`'s gRPC
   adapter file; `grpc_worker` has its gRPC adapter, but not `http_api`'s HTTP
   adapter or its `CreateTaskUseCase` — proving the partitioning actually
   filters per app rather than just copying everything into every crate.
5. Asserts each crate's own `src/main.rs` spawns only its own app's adapter.
6. Patches local git dependencies and runs a real `cargo build --offline`
   against the workspace root — building both independent binaries in one
   shot.

Exits non-zero on the first failure; the scratch directory is removed on exit
either way.
