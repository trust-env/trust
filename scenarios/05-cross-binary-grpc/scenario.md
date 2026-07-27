# Scenario 5 — cross-binary gRPC communication

## Goal

Prove that a use case running in one generated binary can call, over a
**real network gRPC connection**, an adapter running in a *different*
generated binary — the actual "HTTP service load-balancing across gRPC
backend services" capability the multi-app workspace (scenario 04) was
built towards. Unlike every other scenario, this one doesn't stop at
`cargo build` — it starts both compiled binaries as real processes and
makes a real HTTP request that only succeeds if it truly round-trips
through gRPC to the other binary's own Postgres-backed storage.

## What `app.tr` declares

- `Task` — a plain data type, shared by both apps.
- `TaskInfra` — `psql` storage for `Task`, local to `GrpcWorker`.
- `GetTaskUseCase` — `call: TaskInfra.find_one`, exposed by `GrpcWorkerAdapter`'s
  `GetTask` rpc.
- `GrpcWorkerAdapter` — a `grpc {}` adapter, listening on `0.0.0.0:15051`.
- `RemoteTaskInfra` — a **`grpc_client`** infra (new in this phase): backs
  the same `Storage<Task, E>` trait `psql` storage implements, but
  `find_one` makes a real gRPC call to `GrpcWorkerAdapter`'s `GetTask` rpc
  at the static address `127.0.0.1:15051`.
- `GetRemoteTaskUseCase` — `call: RemoteTaskInfra.find_one`, exposed by
  `HttpApiAdapter`'s `GET /tasks/:id` route, listening on `0.0.0.0:18080`.
- `app HttpApi: run { HttpApiAdapter }` / `app GrpcWorker: run {
  GrpcWorkerAdapter }` — two independent binaries, per scenario 04's
  workspace model.

## How to run it

```bash
./scenarios/05-cross-binary-grpc/run.sh
```

The script:

1. Builds the `trust` CLI and runs it against `app.tr`.
2. Asserts both sides of the cross-binary call were generated: `GrpcWorker`'s
   server-side rpc file, and `HttpApi`'s own `Grpc{Entity}Client` (its own
   duplicate `.proto`, in its own crate — no shared crate between the two).
3. Checks `http_api`'s `main.rs` connects `RemoteTaskInfra` to the correct
   static address, not a psql env-var-driven connection.
4. Runs a real `cargo build --offline` against the generated workspace,
   compiling both independent binaries.
5. Starts a throwaway `postgres:16` Docker container (already cached
   locally, so this never needs network access), creates the `tasks` table,
   and seeds one known row directly — entirely independent of either
   generated binary, so the later HTTP response can only match if the value
   really came from this exact row.
6. Starts the compiled `grpc_worker` binary as a real background process
   (pointed at the throwaway Postgres via `TASK_INFRA_DB_URL`), waits for it
   to start listening on `15051`.
7. Starts the compiled `http_api` binary as a real background process,
   waits for it to start listening on `18080`.
8. `curl`s `GET http://127.0.0.1:18080/tasks/1` and asserts the response
   contains the seeded row — the value can only have gotten there via
   `http_api`'s use case calling `RemoteTaskInfra.find_one`, which dials
   `grpc_worker` over gRPC, which queries the real Postgres row.
9. `curl`s `GET http://127.0.0.1:18080/tasks/999` (an id that was never
   seeded) and asserts a `404` — proving the "not found" branch (gRPC
   response has no row → `find_one` returns `None` →
   `GetRemoteTaskUseCase`'s `NotFound` error → HTTP 404) survives a real
   remote call correctly, not just the happy path.
10. Kills both background processes and the Postgres container, and removes
    the scratch directory, in a `trap` — runs on success or failure alike.

Exits non-zero on the first failure, printing both binaries' logs if the
final HTTP call doesn't succeed.
