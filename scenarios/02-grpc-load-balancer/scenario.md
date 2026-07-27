# Scenario 2 — gRPC adapter + a load balancer across separate processes

## Goal

Prove two things about Trust's infrastructure-composition features:

1. A `grpc {}` adapter (generates a `.proto`, a `build.rs`, and a tonic
   service impl) can coexist with a `load_balancer {}` in the same app, and
   the whole generated workspace still compiles.
2. **`load_balancer {}` genuinely distributes load across independently
   deployed processes** — not in-process tokio tasks sharing one adapter
   instance and one DB connection. The proxy (`templates/file/load_balancer.rs`)
   is adapter-agnostic: it just forwards HTTP requests to whatever `targets`
   (real network addresses) it's given. This scenario proves that by
   actually running the *same compiled binary* as 3 separate OS processes,
   each with its own database, and showing the load balancer's responses
   really cycle through all 3 — not one shared instance answering every time.

## What `app.tr` declares

- `User` / `CreateUserRequest` / `GetUserRequest` — plain data types.
- `UserInfra` — `psql` storage for `User`.
- `CreateUserUseCase`, `GetUserUseCase` — one use case per storage call,
  each mapping `DatabaseError`/`ConversionError` to a shared `InternalError`.
- `PublicHttpApi` — an HTTP adapter with `POST /users` / `GET /users/:id`.
- `InternalGrpcApi` — a gRPC adapter exposing the *same* two use cases as
  `CreateUser`/`GetUser` RPCs.
- `app Main: run` — runs `InternalGrpcApi` directly (`0.0.0.0:50051`) and a
  `load_balancer` proxying to 3 **external** target addresses
  (`127.0.0.1:51000/51001/51002`) on `0.0.0.0:19100`. `Main`'s own binary
  never constructs `PublicHttpApi` — it only proxies.
- `app PublicHttpBackend: run { PublicHttpApi }` — a **separate** app/binary.
  You build this once and deploy it to each of the 3 target addresses
  (in this scenario, 3 real OS processes on one host standing in for 3
  servers; in production, 3 actual machines).

## How to run it

```bash
./scenarios/02-grpc-load-balancer/run.sh
```

The script:

1. Builds the `trust` CLI, runs it against `app.tr`.
2. Asserts both apps' generated files exist (gRPC artifacts + load balancer
   in `main/`, the HTTP adapter in `public_http_backend/`).
3. Greps `main/src/main.rs` to confirm it builds `RoundRobinProxy` with the
   3 declared external addresses and **never** constructs `PublicHttpApi`
   locally — pinning that the load balancer is adapter-agnostic.
4. Runs a real `cargo build --offline` against the workspace — both
   binaries, one build.
5. Starts a throwaway Postgres container (Docker) with **4 separate
   databases**: one per backend instance (`backend0`/`1`/`2`, each seeded
   with a `User` row with the *same* id but a different name — `Backend-0`,
   `Backend-1`, `Backend-2`) plus one for `Main`'s own `InternalGrpcApi`.
6. Starts the compiled `public_http_backend` binary **3 times as 3 real,
   separate OS processes** (distinct PIDs), each pointed at its own
   database via `USER_INFRA_DB_URL` and its own port via `PORT`.
7. Starts the compiled `main` binary (the load balancer + gRPC adapter).
8. Calls `GET /users/1` through the load balancer 6 times and asserts all
   3 distinct names (`Backend-0/1/2`) show up — proof the round robin
   really reached 3 separate processes with 3 separate states, not one
   shared instance.
9. Kills all 4 processes and the Postgres container, and removes the
   scratch directory, in a `trap` — runs on success or failure alike.

Exits non-zero on the first failure, printing every process's log if the
round-robin check doesn't see all 3 backends.
