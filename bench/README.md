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

## Noise on a shared machine

A dev machine is rarely idle (builds, tests, other sessions), and anything
scheduled on the app's CPUs steals throughput from it. The bench guards
against that:

- Before each measurement it waits until the rest of the machine uses at most
  `--max-busy` cores (default 2, up to `--quiet-timeout` seconds).
- Every bench container (Postgres, app, oha) runs under one cgroup,
  `trustbench.slice`, so during each measurement the CPU time of *other*
  programs is known exactly: `/proc/stat` user + nice + system time minus
  the slice's usage. It is recorded in three places: the app's CPUs
  (threshold 0.1 core), their hyperthread siblings, which share the app's
  physical cores (0.35 core, as an active desktop already puts ~0.2 there),
  and the Postgres + oha CPUs (0.6 core). Above a threshold the point is
  measured again (`--noisy-retries`, default 2); a point still noisy after
  that is kept, and flagged ⚠ in the dashboard.
- The app, Postgres and oha threads run at `nice -10` (`--nice`): a
  background build at `nice 5` that lands on the bench CPUs then gets about
  1/28th of the CPU instead of half. Only the bench's own containers are
  reniced. The harness itself (this script and the docker CLIs it starts) is
  pinned to the CPUs the bench does not use.
- Results are saved after every contender. `--merge` re-measures only the
  contenders given to `--apps` and keeps the others from `results/latest.json`;
  `--retry-noisy` re-measures only the points still flagged:

```bash
python3 bench/bench.py --apps trust,go --merge   # redo two contenders
python3 bench/bench.py --retry-noisy             # redo the flagged points
```

## Profiling: where the time goes

`bench/profile.py` samples the call stacks of Trust and of the hand-written
axum service under the same load, with `perf` (1999 Hz, frame pointers), and
renders [flame graphs](https://www.brendangregg.com/flamegraphs.html): one
box per function, as wide as the CPU time spent in it and its callees.

```bash
python3 bench/profile.py                       # all scenarios, 64 connections
python3 bench/profile.py --scenarios get --duration 15
```

It writes `dashboard/flame/<app>-<scenario>.svg` (open in a browser, click to
zoom), a differential graph `diff-<scenario>.svg` (red: more time in Trust),
and `dashboard/profile.js`, which the dashboard turns into a CPU breakdown by
component (HTTP, routing, JSON, sqlx, tracing, runtime, kernel), the most
expensive functions, and what Postgres waits on during writes.

`perf` runs in a container that shares the app's PID namespace with
`CAP_PERFMON`, so the host's `perf_event_paranoid` stays untouched. The
profiled images are the bench images rebuilt with frame pointers and line
tables, for both contenders alike.

Even so, compare contenders within one report rather than absolute numbers
across machines.

## Adding a contender

1. Add `apps/<dir>/` with a `Dockerfile` that serves the contract above on
   port `8080`, reading `DATABASE_URL`, `DB_POOL` and (if multi-process)
   `WORKERS`.
2. Add an entry to `APPS` in `bench.py` (id, label, directory, hand-written
   source files, `"workers": True` for one process per CPU).
