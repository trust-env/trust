# Features

Everything Trust can generate today, grouped by theme. Each entry says what
the feature gives you, shows the smallest useful `.tr` snippet, and links
to the full reference in [keywords.md](keywords.md); complete, runnable
projects using them are in [recipes.md](recipes.md) (11 to 22 for the
newer features). Exact generated shapes
are in [conversions.md](conversions.md), CLI flags in [cli.md](cli.md), and
what is *not* supported in [limitations.md](limitations.md).

A `.tr` project compiles to a Cargo workspace of Rust services (axum, sqlx,
tonic) that build and run on their own: the database tables are created at
startup, the configuration comes from the environment, and nothing at
runtime depends on Trust.

## Contents

1. [Projects and files](#1-projects-and-files)
2. [Types](#2-types)
3. [PostgreSQL storage](#3-postgresql-storage)
4. [Other infrastructure](#4-other-infrastructure)
5. [Use cases](#5-use-cases)
6. [Declarative shortcuts](#6-declarative-shortcuts)
7. [HTTP adapter](#7-http-adapter)
8. [MCP adapter (AI agents)](#8-mcp-adapter-ai-agents)
9. [gRPC](#9-grpc)
10. [Time, language and scheduling](#10-time-language-and-scheduling)
11. [Notifications and bots](#11-notifications-and-bots)
12. [Apps, binaries and deployment](#12-apps-binaries-and-deployment)
13. [Compiler tooling](#13-compiler-tooling)
14. [Extending Trust](#14-extending-trust)

---

## 1. Projects and files

| Feature | What it gives you | Reference |
|---|---|---|
| Multi-file projects | `include "./modules/tasks.tr"` makes another file's declarations part of the project; includes are transitive and duplicates are reported with `file:line`. | [`include`](keywords.md#include) |
| File-relative paths | Every path a file writes (`include`, `code { file }`, `seed`, ...) is relative to that file, so a module carries its own Rust files. | [Paths](keywords.md#paths) |
| Typed imports | `import { User, Role } from "./shared.tr"` with `export type ...`. | [`import`](keywords.md#import) |
| Project settings | `config { json_case: "camel" tz: "Europe/Paris" locale: "fr" deps { ... } }`. | [`config`](keywords.md#config) |
| Order-free declarations | Anything can reference anything declared later or in another file. | [Syntax notes](keywords.md#syntax-notes) |
| Errors with locations | Parse and generation errors point to `file:line:col`. | — |

```tr
config { json_case: "camel" tz: "Europe/Paris" locale: "fr" }

include "./modules/tasks/tasks.tr"
include "./modules/inbox/inbox.tr"
include "./adapters/api.tr"
```

## 2. Types

| Feature | What it gives you | Reference |
|---|---|---|
| Scalars | `String`, `Int` (i64), `Float`, `Decimal` (exact, serialized as a string), `Bool`, `Uuid`. | [Type keywords](keywords.md#type-keywords) |
| Time types | `DateTime` (UTC instant), `Date`, `Time`. | [`DateTime`](keywords.md#datetime), [`Date` and `Time`](keywords.md#date-and-time) |
| Containers | `Option<T>`, `List<T>`, `Json`; nested structs and lists are stored as JSONB. | [`Option`](keywords.md#option), [`List`](keywords.md#list), [`Json`](keywords.md#json) |
| Enums | `enum Status { todo doing done }`: text on the wire and in the database, 422 on an unknown value. | [`enum`](keywords.md#enum) |
| Tagged enums | Variants with fields: `{"type": "moveEvent", ...}`, stored as JSONB. | [Tagged enums](keywords.md#tagged-enums) |
| Validated text | `Regex<email>`, `Regex<"^[A-Z]{3}$">`: 422 when a request doesn't match. | [`Regex`](keywords.md#regex) |
| Number ranges | `Int(1..4)`, `Float(0..60)`, `Int(0..)`: 422 out of bounds, bounds in the schemas. | [Number ranges](keywords.md#number-ranges) |
| Derived types | `Omit<Task, id>`, `Pick<...>`, `Partial<...>`, and `Task + { extra: Int }`. | [Derived types](keywords.md#derived-types) |
| Conversions | `map InboxItem -> Task { title: title  due: suggestion.due }` gives `item.to_task()`. | [`map`](keywords.md#map) |

```tr
enum Priority { low normal urgent }
type Task {
    id: Int
    user_id: Int
    title: String
    priority: Int(1..4)
    tags: List<String>
    due: Option<DateTime>
}
type NewTask = Omit<Task, id, user_id>
```

## 3. PostgreSQL storage

```tr
infra TaskInfra {
    storage {
        psql {
            entity: Task
            owner: user_id
            unique: [user_id, title]
            index: [due]
            references { project_id: ProjectInfra set_null }
            defaults { status: "todo" priority: 3 domain: guess_domain(title) }
            stamps { done_at: status == "done" }
        }
    }
}
```

| Feature | What it gives you | Reference |
|---|---|---|
| Generated storage | `save_one`, `find_one`, `list`, `update_one`, `patch_one`, `upsert_one`, `save_or_patch`, `delete`, and the `*_many` variants. | [`psql`](keywords.md#psql) |
| Automatic migrations | The binary creates tables and adds new columns at startup; `renamed {}` and `dropped: [...]` handle renames and removals. | [`psql`](keywords.md#psql), [Schema options](keywords.md#schema-options) |
| Seed data | `seed: "./seed/sizes.sql"`, re-run (idempotently) at every start. | [`seed`](keywords.md#seed) |
| Constraints | `unique`, `index`, foreign keys with `cascade`, `set_null` or `restrict`. | [Schema options](keywords.md#schema-options) |
| Creation defaults | Literals, `now()`, `today()`, or a call into your own Rust (`guess_domain(title)`). Creation inputs make those fields optional. | [Creation defaults and stamps](keywords.md#creation-defaults-and-stamps) |
| Derived timestamps | `stamps { done_at: status == "done" }` is set on entering the state, kept, and cleared on leaving it. | [Creation defaults and stamps](keywords.md#creation-defaults-and-stamps) |
| Ownership | `owner: user_id`: rows belong to the calling user, and other users' rows read as 404. | [`owner`](keywords.md#owner) |
| Shared pools | `database Main { url pool }`; infras on the same URL share one pool. | [`database`](keywords.md#database) |
| Transactions | `transaction: true` on a use case, or `begin()` / `in_transaction(&tx)` from code. | [`transaction`](keywords.md#transaction) |
| Query builder | From code: `query(Query::new().eq("user_id", user).gt("end_at", from).order_by("start_at"))`. | [`code`](keywords.md#code) |
| Row-or-default | `get_or_default("user_id", user)`: the owner's row, or a fresh one from the defaults. | [`patch_one`, `upsert_one` and `save_or_patch`](keywords.md#patch_one-upsert_one-and-save_or_patch) |

## 4. Other infrastructure

| Feature | What it gives you | Reference |
|---|---|---|
| Object storage | `s3 { bucket entity }` for MinIO and other S3-compatible services: presigned upload and download URLs, and deletion. | [`s3`](keywords.md#s3), [`blob`](keywords.md#blob) |
| Outbound HTTP | `http_client { base_url headers retry timeout }`: JSON, text and form helpers, retries with backoff. | [`http_client`](keywords.md#http_client) |
| OAuth2 clients | `oauth2 { token_url client_id client_secret refresh_token }`: a bearer token on every request, cached and renewed (including after a 401). | [`http_client`](keywords.md#http_client) |
| Paginated APIs | `get_json_paged(path, "items", "nextPageToken")` collects every page. | [`http_client`](keywords.md#http_client) |
| Calendar feeds | `ics_feed { url timezone_fallback }`: `occurrences(from, to)` with recurrence rules, exceptions, moved and cancelled events, and IANA plus Windows time zones. | [`ics_feed`](keywords.md#ics_feed) |
| Cross-service calls | `grpc_client`: another Trust app's gRPC adapter, used like a storage. | [`grpc_client`](keywords.md#grpc_client) |

```tr
infra GoogleCalendar { http_client {
    base_url: "https://www.googleapis.com/calendar/v3"
    oauth2 { token_url: "https://oauth2.googleapis.com/token" client_id: "${GOOGLE_CLIENT_ID}"
             client_secret: "${GOOGLE_CLIENT_SECRET}" refresh_token: "${GOOGLE_REFRESH_TOKEN}" }
    retry: 3
} }
infra OutlookIcs { ics_feed { url: "${OUTLOOK_ICS_URL}" timezone_fallback: "Europe/Paris" } }
```

## 5. Use cases

A use case is one operation of the application. It has five forms, which
can't be mixed:

| Form | When to use it | Reference |
|---|---|---|
| `call: TaskInfra.list` | Storage operations, with nothing written by hand. Several `call:` lines run in order. | [`call`](keywords.md#call) |
| `logic { ... }` | A body in Trust's small typed scripting language: checked by `trust`, limited to the project's infras and use cases, translated to Rust. | [`logic`](logic.md) |
| `code { file \| module ... }` | Your own Rust, with every surrounding piece generated (errors, wiring, deps). | [`code`](keywords.md#code) |
| `blob: MediaInfra.presigned_put_url` | One object-storage operation. | [`blob`](keywords.md#blob) |
| `ai { prompt: '...' }` | A body written once by an LLM at compile time. | [`ai`](keywords.md#ai) |

Options of `call:` use cases:

| Feature | What it gives you | Reference |
|---|---|---|
| Guessed inputs | No `input:` needed: Trust derives it from the call (create, patch, list...). | [`input`](keywords.md#input) |
| Filters | `status in ["todo", "doing"]`, `priority >= input.min`, `title contains input.q`, `due < now()`, `day: today()`. Optional inputs filter only when set. | [`filters`](keywords.md#filters) |
| Sorting and paging | `order_by: priority desc, due`; `limit` and `offset` on every list. | [`order_by`](keywords.md#order_by), [`limit` / `offset`](keywords.md#limit--offset) |
| Error mapping | `errors { DatabaseError -> Conflict("already exists") }`, then HTTP status mapping per route. | [`errors`](keywords.md#errors), [`status`](keywords.md#status) |
| Partial updates | `patch_one`: only the fields sent, and `null` clears. `save_or_patch`: creates without an id, patches with one. | [`patch_one`, `upsert_one` and `save_or_patch`](keywords.md#patch_one-upsert_one-and-save_or_patch) |
| Shaped outputs | `output:` may keep some of the entity's fields and add `action_id`. | [`output`](keywords.md#output) |

`logic {}` use cases get:

| Feature | What it gives you | Reference |
|---|---|---|
| Checked before the build | Types, fields, enum values, errors and every return path, reported at `file.tr:line:col`. | [`logic`](logic.md) |
| Storage and APIs | `TaskInfra.get(id)`, `query(status != Status.done, order_by: due)`, `create`, `update`, `delete`; `Api.post(path, body)` decoded into the expected type. Owner-scoped when the infra has `owner:`. | [Infras](logic.md#infras) |
| Safe by construction | No file/process/env/network access beyond declared infras (`env("NAME")` takes a literal), no `while`, no `null`, no panic on division or list access. | [Why not Rust](logic.md#why-not-rust) |
| Typed errors | `fail AlreadyDone("...")` adds the variant; routes map it with `status {}`. | [Statements](logic.md#statements) |

Hand-written `code {}` use cases get:

| Feature | What it gives you | Reference |
|---|---|---|
| Real Rust modules | `module: "./plan.rs"` with `pub async fn execute(deps, user, input)`, helpers and `#[cfg(test)]` tests. | [`code`](keywords.md#code) |
| Shared code | `include: "./shared/priority.rs"`, visible from every body. | [`code`](keywords.md#code) |
| Dependencies | `infra: [A, B]`, `infra_set DayData { ... }`, `use_case: [Other]`, `deps { rrule: "0.14" }`, custom `errors { ... }`. | [`code`](keywords.md#code) |
| Raw requests | `input: RawRequest`: the exact body and headers, e.g. to verify a webhook signature. | [`RawRequest`](keywords.md#rawrequest) |

```tr
use_case PlanWeekUseCase {
    input: PlanRequest
    output: PlanResult
    transaction: true
    journal: Actions
    code {
        module: "./plan_week.rs"
        include: "../shared/planner.rs"
        infra: [TaskInfra, EventInfra]
        use_case: [ListSettingsUseCase]
    }
}
```

## 6. Declarative shortcuts

Blocks that generate several use cases, routes and types at once.

| Feature | What it gives you | Reference |
|---|---|---|
| Resources | `resource Tasks { infra path list create get patch delete }`: CRUD use cases and REST routes in one block. | [`resource`](keywords.md#resource) |
| Singleton resources | `singleton default { ... } get upsert`: one row per user (settings), with defaults. | [`resource`](keywords.md#resource) |
| Undo journal | `journal Actions { ... undo: "/actions/:id/toggle" }`: every write of a journaled use case is recorded, and can be undone or redone. A row already in its target state isn't a conflict. | [`journal`](keywords.md#journal) |
| Sync mirrors | `mirror X { source into key scope skip map window }`: copies an ICS feed or another use case's items into a table, upserting on a key and deleting what disappeared; `skip {}` leaves items out (`item.title matches "^Présence"`). Reports `{ configured, written, removed, error }`. | [`mirror`](keywords.md#mirror) |

```tr
resource Tasks { infra: TaskInfra path: "/tasks" journal: Actions list create get save patch delete }
resource Settings { infra: SettingsInfra path: "/settings" owner: user_id singleton default { margin_min: 10 } get upsert }

mirror OutlookMirror {
    source: OutlookIcs
    into: EventInfra
    key: [user_id, kind, external_id]
    scope { kind: "outlook"  end_at > window.from  start_at < window.to }
    map { kind: "outlook"  external_id: item.instance_id  title: item.title  start_at: item.start  end_at: item.end }
    window { from: week_start  days: 28 }
}
```

## 7. HTTP adapter

```tr
adapter Api {
    http {
        on: "0.0.0.0:${PORT:-8080}"
        cors: "${CORS_ORIGINS}"
        auth: required
        events: "/events"
        resources { Tasks Settings Actions TelegramBot }
        routes {
            POST "/plan" { use_case: PlanWeekUseCase status { Conflict -> 409 } }
            GET "/public/stats" { use_case: StatsUseCase auth: none }
        }
    }
}
```

| Feature | What it gives you | Reference |
|---|---|---|
| Routes | `GET/POST/PUT/PATCH/DELETE`; path parameters (`:id`), query strings and bodies are merged into the input. | [`routes`](keywords.md#routes), [`body`](keywords.md#body) |
| Status mapping | Per-route `status { Error -> 409  _ -> 502 }`; 422 for invalid bodies, 400 for malformed JSON. | [`status`](keywords.md#status) |
| Authentication | `auth: required` (JWT), `auth { secret header user }` (machines), or permission rows; set once for the adapter, `auth: none` opts out. | [`auth`](keywords.md#auth), [Defaults](keywords.md#defaults) |
| CORS | `cors: "${CORS_ORIGINS}"`. | [`cors`](keywords.md#cors) |
| Live updates | `events: "/events"`: a Server-Sent Events stream of every write, filtered per user. | [HTTP adapter options](keywords.md#http-adapter-options) |
| Response shape | `envelope: false` returns entities and lists bare; `json_case: "camel"` names fields in camelCase. | [HTTP adapter options](keywords.md#http-adapter-options) |
| Operations | `GET /health`, structured request logs. | [HTTP adapter options](keywords.md#http-adapter-options) |

## 8. MCP adapter (AI agents)

| Feature | What it gives you | Reference |
|---|---|---|
| Tools | `tools { capture { use_case description } }`: each use case becomes an MCP tool with a JSON schema; errors come back readable. | [`mcp`](keywords.md#mcp) |
| Resources | `resources { goals { use_case uri description } }`: read-only context for the agent. | [`mcp`](keywords.md#mcp) |
| Transports | Streamable HTTP (`POST /mcp`) and stdio (`<binary> mcp`). | [`mcp`](keywords.md#mcp) |
| Remote clients | `oauth { public_url password user }`: an OAuth 2.1 server (discovery, dynamic registration, PKCE, refresh tokens), e.g. for claude.ai connectors. | [`mcp`](keywords.md#mcp) |

```tr
adapter Agent {
    mcp {
        on: "127.0.0.1:${MCP_PORT}"
        auth: required
        oauth { public_url: "${MCP_PUBLIC_URL}" password: "${MCP_OAUTH_PASSWORD}" user: 1 }
        tools { capture { use_case: CaptureUseCase description: "Add an item to the inbox" } }
        resources { goals { use_case: ListGoalsUseCase uri: "world://goals" description: "Every goal" } }
    }
}
```

## 9. gRPC

| Feature | What it gives you | Reference |
|---|---|---|
| gRPC adapter | `grpc { services { rpc GetUser { use_case } } on }`: a `.proto` file plus a tonic server, with `protoc` vendored. | [`grpc`](keywords.md#grpc), [`rpc`](keywords.md#rpc) |
| gRPC client storage | Another app's gRPC service, called like a storage (`find_one`). | [`grpc_client`](keywords.md#grpc_client) |

## 10. Time, language and scheduling

| Feature | What it gives you | Reference |
|---|---|---|
| App time zone | `config { tz: "Europe/Paris" }`: `crate::tz::now_local()`, `today()`, `day_bounds`, `week_bounds`, `local_to_utc` (daylight-saving safe). `today()` in filters and defaults, and schedules, use the zone. | [`config`](keywords.md#config) |
| Formatting | `config { locale: "fr" }` (or `en`): `fmt_date(t, "EEEE d MMMM")`, `fmt_time`, `fmt_relative_day`, `plural(n, "réunion", "réunions")`. | [`config`](keywords.md#config) |
| Scheduled jobs | `schedule Jobs { "30 7 * * *" => SendBriefUseCase { send: true } }`: cron with time zones, run inside the app. | [`schedule`](keywords.md#schedule) |

```tr
schedule Jobs {
    user: 1
    "*/15 * * * *" => OutlookMirrorUseCase
    "30 7 * * *" => DailyMessageUseCase { send: true }
}
app Main: run { Api Agent Jobs }
```

## 11. Notifications and bots

| Feature | What it gives you | Reference |
|---|---|---|
| Channels | `channel Telegram { telegram { token chat } }`, `channel Team { webhook { url format: discord } }`. | [`channel` and `notifications`](keywords.md#channel-and-notifications) |
| Routed notifications | `notifications Notify { infra default fallback: log routing { conflict: [Team, Telegram] } }`: `deps.notify().send(kind, text)` tries the channels in order, and `send_recorded(...)` also stores the row. | [`channel` and `notifications`](keywords.md#channel-and-notifications) |
| Telegram bots | `bot TelegramBot { channel path secret user commands on_reply default reply }`: the webhook, the secret check, routing of commands, replies and free text to use cases, and an acknowledgment. | [`bot`](keywords.md#bot) |

```tr
channel Telegram { telegram { token: "${TELEGRAM_BOT_TOKEN}" chat: "${TELEGRAM_CHAT_ID}" } }
notifications Notify { infra: NotificationInfra default: Telegram fallback: log }
bot TelegramBot {
    channel: Telegram  path: "/webhooks/telegram"  secret: "${TELEGRAM_WEBHOOK_SECRET}"  user: 1
    commands { journal: AgentJournalUseCase  inbox: CaptureUseCase }
    on_reply { review: AgentJournalUseCase }
    default: CaptureUseCase
    reply: "{result.summary}"
}
```

## 12. Apps, binaries and deployment

| Feature | What it gives you | Reference |
|---|---|---|
| Apps | `app Main: run { Api Agent Jobs }`: one binary per app, several apps in one workspace. | [`app`](keywords.md#app), [`run`](keywords.md#run) |
| Load balancer | `load_balancer { targets on }`: a round-robin reverse proxy. | [`load_balancer`](keywords.md#load_balancer) |
| Configuration | `${VAR}` and `${VAR:-default}` everywhere addresses and secrets go; `.env` is loaded at startup. | [Defaults](keywords.md#defaults), [`on`](keywords.md#on) |
| Service tokens | `<binary> token <user_id> [days]` prints a JWT for scripts and agents. | [cli.md](cli.md#generated-binary-commands) |
| Offline builds | `--vendor`: git dependencies are copied into the output, so it builds without network access (Docker, CI). | [cli.md](cli.md) |

## 13. Compiler tooling

| Feature | What it gives you | Reference |
|---|---|---|
| Dev mode | `--dev`: a live `GET /app` documentation page and a scaffolded `.env`. | [cli.md](cli.md#dev-mode---dev) |
| Typed clients | `--emit-ts=client.ts` (TypeScript types and one client class per adapter) and `--emit-openapi=api.json`. | [cli.md](cli.md) |
| Output location | `--out=<dir>`, refusing directories it shouldn't wipe. | [cli.md](cli.md) |
| Debugging | `--verbose` prints tokens and the AST. | [cli.md](cli.md) |
| Tests in generated code | `#[cfg(test)]` tests of `module:` files run with `cargo test` in the generated crate. | [`code`](keywords.md#code) |
| End-to-end scenarios | `scenarios/*/run.sh`: CRUD, gRPC + load balancer, AI + plugin, multi-app, cross-binary gRPC, run live. | `scenarios/README.md` |

## 14. Extending Trust

| Feature | What it gives you | Reference |
|---|---|---|
| Plugins | `--plugin=/path/libplugin.so`: a new top-level keyword with its own templates, over a C ABI and JSON. | [cli.md](cli.md) |
| AI-written bodies | `ai { prompt }` with `--ai-api-key` (Anthropic), generated once at compile time. | [`ai`](keywords.md#ai) |

---

## Good to know before you start

- `./out` (or `--out`) is wiped and regenerated on every compile: never
  hand-edit generated code. Hand-written logic lives in `code {}` files next
  to your `.tr` files.
- The runtime crates (`common`, `auth`) come from Trust's own repository
  (`runtime/`) over HTTPS; `--vendor` makes the output buildable offline.
- What Trust doesn't do yet: [limitations.md](limitations.md).
