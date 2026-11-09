# Scenario 1 — CRUD HTTP + PSQL storage

## Goal

Prove the core, most-used pipeline of the Trust language end to end: a
`type` becomes a Postgres-backed entity, an `infra {}` block generates CRUD
storage for it, `use_case {}` blocks wire storage calls to input/output
types, and an `adapter { http { ... } }` block exposes those use cases as
real HTTP routes — and the whole generated Rust project actually compiles.

This is the "everyday" shape most `.tr` files will have, so it's the first
thing that must never regress.

## What `app.tr` declares

- `Task` / `CreateTaskRequest` / `GetTaskRequest` — plain data types.
- `TaskInfra` — a `psql` storage block for `Task` (generates the 8 CRUD
  storage methods: `save_one`, `save_many`, `find_one`, `find_many`, `list`,
  `update_one`, `delete`, `delete_many`).
- `CreateTaskUseCase`, `GetTaskUseCase`, `ListTasksUseCase` — one use case per
  storage call (`save_one`, `find_one`, `list`).
- `HttpAdapter` — an HTTP adapter routing `POST /tasks`, `GET /tasks/:id`,
  `GET /tasks` to those three use cases.

## How to run it

```bash
./scenarios/01-crud-http-psql/run.sh
```

The script:

1. Builds the `trust` CLI (`cargo build --bin trust`).
2. Runs it against `app.tr` in a scratch temp directory (with `mod/`
   symlinked in, for the dependency patching in step 4).
3. Asserts the expected generated files exist (domain type, psql storage,
   the 3 use cases, the HTTP adapter + its route handlers).
4. Patches the generated `Cargo.toml`'s private git dependencies
   (`common`, ...) to point at this repo's local `mod/*` checkouts, then
   runs a real `cargo build --offline` against the generated project.

Exits non-zero (and prints the relevant log) on the first failure. On
success, prints a `PASS` line for each check. The scratch directory is
removed on exit either way.
