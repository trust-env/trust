# Trust Bench

Measures what a Trust-generated service costs at runtime, next to the same
API written by hand in other stacks, and shows it in a dashboard.

| Contender | Stack | Source |
|---|---|---|
| `trust` | Trust → axum + sqlx (generated) | [`apps/trust/app.tr`](apps/trust/app.tr) |
| `rust-axum` | Rust, axum + sqlx, hand-written | [`apps/rust-axum`](apps/rust-axum) |
| `go` | Go, `net/http` + pgx | [`apps/go`](apps/go) |
| `fastify` | Node.js, Fastify + `pg` | [`apps/fastify`](apps/fastify) |
| `nestjs-express` | NestJS 10 on Express (its default) + `pg` | [`apps/nestjs`](apps/nestjs) |
| `nestjs-fastify` | NestJS 10 on Fastify + `pg` | [`apps/nestjs`](apps/nestjs) |
| `fastapi` | Python, FastAPI + asyncpg on uvicorn | [`apps/fastapi`](apps/fastapi) |

Every contender serves the same contract, checked before it is measured:

| Scenario | Request | Response |
|---|---|---|
| `ping` | `GET /ping` | `{"ok":true}` |
| `get` | `GET /users/:id` (random id, 10k rows) | `{"user":{...}}`, 404 when missing |
| `list` | `GET /users?limit=20&offset=N` | `{"users":[...]}` ordered by id |
| `create` | `POST /users` | `{"user":{...}}` |

## Run

Needs Docker (cgroup v2), Python 3 and the Rust toolchain (to build the
`trust` compiler). Nothing else is installed on the host.

```bash
python3 bench/bench.py                 # all contenders, all scenarios, 16/64/256 connections
python3 bench/bench.py --no-cache      # also time cold image builds
python3 bench/bench.py --apps trust,rust-axum,nestjs-express --scenarios get,create \
    --connections 64 --duration 5      # a quick comparison
```

Then open [`dashboard/index.html`](dashboard/index.html) in a browser. Each run
writes `results/run-<date>.json`, `results/latest.json` and `dashboard/data.js`;
the dashboard can also load any older `results/*.json` file.

`python3 bench/bench.py --help` lists every option (CPU sets, memory limit,
pool size, durations).

## What is measured

- **Throughput and latency** (p50 / p90 / p99 / p99.9) from
  [oha](https://github.com/hatoo/oha), after a warm-up.
- **CPU** used by the app container, read from its cgroup (`cpu.stat`), and
  **req/s per core**, which stays meaningful when the load generator, not the
  app, is the bottleneck.
- **Peak memory** during each run and **idle memory** after start-up
  (`memory.current`).
- **Start-up time**: `docker run` to the first `200` on `/ping`, including
  Trust's migrations.
- **Image size**, **build time**, and **lines of code written by hand**
  (non-blank, non-comment lines of the source files listed in `bench.py`;
  for Trust, `app.tr` + `ping.rs`, plus the number of lines it generates).

## Fairness rules

- Same machine, same Postgres, same table, same queries, same pool size
  (`--db-pool`, split across processes for Node and Python).
- CPU pinning: the app, Postgres and the load generator each get their own
  CPUs (defaults suit a 22-thread Intel Core Ultra: app on two distinct
  P-cores without their hyperthread siblings, Postgres on three P-cores,
  oha on the E-cores). Adjust `--app-cpus`, `--pg-cpus`, `--load-cpus` for
  another machine (`lscpu -e` shows the cores).
- Node.js and Python run one process per app CPU (`node:cluster`, uvicorn
  workers), so single-threaded runtimes get the same cores as the others.
- Trust runs with its generated defaults (`info` logging, HTTP tracing layer),
  the way it ships. The hand-written Rust baseline has neither.
- No ORM in the Node contenders: NestJS with TypeORM or Prisma would be slower.

Results from a laptop are noisy: compare contenders within one run rather
than absolute numbers across machines.

## Adding a contender

1. Add `apps/<dir>/` with a `Dockerfile` that serves the contract above on
   port `8080`, reading `DATABASE_URL`, `DB_POOL` and (if multi-process)
   `WORKERS`.
2. Add an entry to `APPS` in `bench.py` (id, label, directory, hand-written
   source files, `"workers": True` for one process per CPU).
