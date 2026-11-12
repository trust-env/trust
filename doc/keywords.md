# Keywords

This page lists all language keywords currently recognized by Trust, what
each one does, and how to use it. For exact type/name conversion rules and
generated-code shapes, see [conversions.md](conversions.md); for full CLI
usage see [cli.md](cli.md).

## Syntax Notes

These apply across the whole language and aren't tied to one keyword:

- **Projects span files.** `include "./x.tr"` and file-relative paths; see
  [`include`](#include).
- **`//` line comments.** Everything from `//` to the end of the line is
  ignored (outside string literals). There is no `#` or `/* */` form.
- **Contextual words.** Newer constructs (`enum`, `schedule`, `config`,
  `mcp`, `order_by`, `transaction`, `notify`, `unique`, ...) are matched by
  position, not reserved — they stay usable as field names.
- **Struct field separators are optional.** Commas between `type { ... }`
  fields may be present or omitted; both are accepted.
- **HTTP verbs (`GET`/`POST`/`PUT`/`PATCH`/`DELETE`) are not reserved
  keywords.** They lex as plain identifiers and are matched by name when
  parsing a route. Any other word in that position is a parse error
  (`Unknown HTTP method: ...`).
- **Numbers are not a distinct token type.** Integer literals (e.g. HTTP
  status codes) lex as identifiers and are parsed as numbers contextually;
  a non-numeric word where a number is expected is a parse error.
- **Declaration order never matters.** `type`, `infra`, `use_case`,
  `adapter`, and `app` blocks can reference each other regardless of which
  one appears first in the file — a two-pass resolution (`build_lookup_context`)
  registers every name before any file is actually generated, and this is
  regression-tested to produce byte-identical output either way. Order
  declarations however reads best to you.
- **Adapter `on:` (not load-balancer `targets`) supports `${VAR}`
  interpolation.** See the [`on`](#on) entry below — this is a common point
  of confusion, so it's called out twice.
- **Undefined type references fail at codegen, not at parse time.** A typo'd
  type name in a field lexes and parses fine; you only find out when you
  actually compile ("Undefined type reference: ..."). Types may reference
  types declared later in the file.

## Core Structural Keywords

### `type`
Defines a type alias or struct.

```tr
type UserId = Int

type User {
    id: Int,
    name: String
}
```

Trailing commas and inter-field commas are both optional — `id: Int name: String`
on separate lines with no commas is equally valid. Every field becomes a
`pub` field on the generated struct; there's no visibility control.

### `export`
Marks a struct declaration as importable by name from another `.tr` module.
Either as a prefix — `export type Task { ... }`, `export enum Domain { ... }` —
or inside the body (`export Task`). On other declarations the prefix is
accepted and changes nothing (use `include` to share them).
Has **zero** effect on the type's own generated code (same derives, same
visibility) — it only changes whether another file's `import` can find it.

```tr
type PublicUser {
    id: Int,
    name: String
    export PublicUser
}
```

### `include`
Makes every declaration of another `.tr` file part of this project — types,
enums, infras, use cases, adapters, schedules, apps:

```tr
config { json_case: "camel" }

include "./modules/tasks/tasks.tr"
include "./modules/inbox/inbox.tr"
include "./adapters/api.tr"
```

Includes are transitive, a file included twice counts once, and two
declarations with the same name are an error naming both `file:line`.

### Paths
Every path a `.tr` file writes — `include`, `import`, `code { file include }`,
`psql { seed }` — is relative to **that file**, so a module can say
`file: "./capture.rs"`. (A path that only exists relative to the working
directory still works, for projects written before this rule.)

### `import`
Imports exported items from another module.

```tr
import PublicUser from "./shared.tr"
import { User, Role } from "./shared.tr"
```

Only local path imports (`"./shared.tr"`) actually work end to end. An
imported type resolves against its own module, and the types it mentions
(an enum, a nested struct) are generated with it.
`import X from "git@github.com:org/repo.git"` **parses** successfully but
codegen hard-errors with "Git imports are not supported yet" the moment it
tries to resolve one — despite this exact form appearing in the top-level
README's own example. Don't rely on git imports today.

### `from`
Used in import declarations to specify the source module.

## Type Keywords

### `String`
Built-in text type → Rust `String`.

### `Uuid`
Built-in UUID type → Rust `Uuid` (from the `uuid` crate, added automatically
with the `serde` feature).

### `Int`
Built-in integer type → Rust `i64` (always 64-bit, never `i32`).

### `Float`
Built-in floating-point type → Rust `f64`. No extra dependency, no extra
`sqlx` feature — Postgres `DOUBLE PRECISION`/`REAL` map to `f64` natively.
**HTTP/`psql` only** — same gRPC gap as `Uuid` (see [`rpc`](#rpc) below):
using a `Float` field on a gRPC-exposed type is a generation-time error.

### `Decimal`
Built-in fixed-point type → Rust `Decimal` (from the `rust_decimal` crate,
added automatically with the `serde-str` feature). Use this instead of
`Float` for anything money-shaped — `f64` has real rounding drift that's a
known source of billing bugs; `Decimal` doesn't.

- **Serializes as a JSON string, not a number** (`"amount": "19.99"`, not
  `"amount": 19.99`) — a deliberate choice (`serde-str`) so precision never
  gets silently lost round-tripping through a JS/JSON client's float type.
- If the type is also used as a storage `entity:`, the crate's `sqlx`
  dependency additionally gets the `rust_decimal` feature (same pattern as
  `Uuid`'s `uuid` feature), mapping to Postgres `NUMERIC`.
- **HTTP/`psql` only**, same gRPC gap as `Uuid`/`Float` — see
  [`rpc`](#rpc) below.

### `Bool`
Built-in boolean → Rust `bool`, Postgres `BOOLEAN`. **HTTP/`psql` only**
(gRPC-exposed types still only support `String`/`Int`).

### `DateTime`
Built-in UTC timestamp → Rust `chrono::DateTime<Utc>` (the `chrono` crate
is added with its `serde` feature, plus the `sqlx` `chrono` feature when
the type is a storage entity), Postgres `TIMESTAMPTZ`. Serializes as an
RFC 3339 string. **HTTP/`psql` only.**

When a `save_one`/`update_one` call builds an entity from an input that
doesn't carry one of its `DateTime` fields (the usual `created_at` case),
that field is set to `chrono::Utc::now()` instead of a default — see
[conversions.md §5](conversions.md#5-use_case-multi-call-semantics).

### `Option`
`Option<T>` — a nullable field → Rust `Option<T>`, a nullable column in
the generated migration. `T` is any other type (not another `Option`).
`Option<Decimal>`, `Option<DateTime>`, `Option<SomeDeclaredType>` all
work; imports follow `T`. **HTTP/`psql` only.**

```tr
type Order {
    id: Int,
    paid: Bool,
    purchase_id: Option<String>,
    created_at: DateTime
}
```

### `List`
`List<T>` → Rust `Vec<T>`, a JSON array. In a `psql` entity it is stored as
`JSONB` (through `sqlx::types::Json`), like a nested struct field.

```tr
type Task {
    id: Int
    tags: List<String>
    steps: Option<List<Step>>
}
```

### `Json`
Any JSON value → `serde_json::Value`, stored as `JSONB`. For payloads whose
shape varies (undo data, channel settings).

### `Date` and `Time`
A calendar date / a wall-clock time without zone → `chrono::NaiveDate` /
`chrono::NaiveTime`, Postgres `DATE` / `TIME`, JSON `"2026-11-10"` /
`"07:30:00"`.

### `enum`
A closed set of values:

```tr
enum TaskStatus { todo doing done "in-review" }
```

Each value is written as it appears on the wire and in the database (an
identifier or a string literal); the Rust variant is its UpperCamelCase form
(`InReview`). The generated enum serializes to that value, implements
`Display`/`FromStr` (`as_str()`, `ALL`), defaults to its first value, and is
stored as `TEXT`. An unknown value in a request is a 422.

#### Tagged enums
Variants may carry fields; the enum is then a tagged union, `{"type":
"<variant>", ...fields}` in JSON, stored as `JSONB`:

```tr
enum ActionPayload {
    acceptInbox { inbox_id: Int  task_id: Int }
    moveEvent   { event_id: Int  before: TimeRange  after: TimeRange }
    reset
}
```

Rust gets `ActionPayload::AcceptInbox { inbox_id, task_id }` (match on it),
`kind()` (the wire name) and a `Default` (the first variant, fields at their
defaults). With `json_case: "camel"` the fields are camelCase too.

### Nested structs in entities
A field whose type is another declared struct is stored as `JSONB`.

### Number ranges
`Int(1..4)`, `Float(0..60)`, `Int(0..)` — a number checked when decoded (a
value out of bounds is a 422 naming the bound); the bounds also reach MCP /
OpenAPI schemas.

### `Regex`
A `String` validated against a pattern whenever it is decoded from JSON
(request bodies, query strings): a mismatch is a 422.

```tr
type ApiKey {
    id: Int,
    label: Regex<slug>,
    code: Option<Regex<"^[A-Z]{3}-[0-9]+$">>
}
```

The argument is a pattern string or a preset: `slug`, `email`, `alpha`,
`alphanumeric`, `numeric`, `hex`, `identifier`. Patterns are checked when
Trust compiles (an invalid one is an error). Validation lives in the serde
layer: values built in Rust code (`code {}` bodies) or read from the
database are not re-checked.

### `entity`
Specifies the entity type for storage infra. For `psql`/`grpc_client`, the
entity name (snake_cased, pluralized with a trailing `s`) becomes both the
SQL table name (`psql`) and the output struct's collection field name for
list-shaped results. For [`s3`](#s3), `entity:` is metadata-only — it names
the generated storage struct (`S3{Entity}Storage`) but has no table/list
semantics, since blob operations always take a fixed `{ key: String }`
shape (see [`blob`](#blob)) rather than an entity-driven one.

## Infrastructure Keywords

### `infra`
Declares an infrastructure service.

### `storage`
Declares storage-backed infra.

### `psql`
PostgreSQL storage backend.

```tr
infra TaskInfra {
    storage {
        psql {
            entity: Task
        }
    }
}
```

Generates a full `Storage<Task, PsqlTaskStorageError>` implementation with
all 8 CRUD/list methods (`save_one`, `save_many`, `find_one`, `find_many`,
`list`, `update_one`, `delete`, `delete_many`) — see
[conversions.md](conversions.md#3-storage-trait-shape-psql) for exact
signatures.

The entity's table is **created by the generated binary itself** at startup:
every `psql` entity gets a `migrations/<table>.sql` in its crate
(`CREATE TABLE IF NOT EXISTS`, then one `ADD COLUMN IF NOT EXISTS` per
field so fields added later reach existing tables) that `main.rs` runs
right after connecting. `id: Int` becomes `BIGSERIAL PRIMARY KEY`. See
[conversions.md §3c](conversions.md#3c-migrations-psql).

#### `seed`
Optional SQL file run at every startup, right after the table exists —
reference data such as sizes, categories or an admin permission row:

```tr
infra SizeInfra {
    storage {
        psql {
            entity: Size
            seed: "./seed/sizes.sql"
        }
    }
}
```

The path is read at compile time relative to the working directory and
copied into the crate as `migrations/<table>.seed.sql`. Because it re-runs
on every boot it **must be idempotent** (`INSERT ... ON CONFLICT DO
NOTHING`, explicit ids, ...).

#### Schema options

```tr
infra EventInfra {
    storage {
        psql {
            entity: CalendarEvent
            unique: [calendar, external_id]
            index: [start_at]
            references { task_id: TaskInfra set_null }
            renamed { start: start_at }
            dropped: [legacy_field]
        }
    }
}
```

| Option | Effect |
|---|---|
| `unique: [a, b]` (repeatable) | `CREATE UNIQUE INDEX`. The first key is what `upsert_one` conflicts on. |
| `index: [a, b]` (repeatable) | An extra index (Trust already indexes the columns its own queries filter and sort on). |
| `references { field: Infra [cascade \| set_null \| restrict] }` | A foreign key to the other psql infra's `id`, added once every table exists. Default: `set_null` for an `Option<Int>` column, `restrict` otherwise. The column is indexed. |
| `renamed { old: new }` | Renames the column on existing databases, before new columns are added, so data follows the rename. `new` must be a field, `old` must not. |
| `dropped: [a]` | `DROP COLUMN IF EXISTS` — data in that column is deleted. |

Foreign keys and transactions need every infra involved on the same
database.

#### Creation defaults and stamps

```tr
psql {
    entity: Task
    defaults {
        status: "todo"  priority: 3  energy: "medium"
        domain: guess_domain(title, now())     // a function of an include: file
    }
    stamps { done_at: status == "done" }       // or: status in ["done", "archived"], status != "x"
}
```

- `defaults {}` fill the fields a creation doesn't give (`save_one`,
  `upsert_one`, `save_or_patch` without id, `get_or_default`): a literal
  (checked against the field's type), `now()`, `today()`, or a call to a
  `pub fn` of a `code { include: ... }` file, field arguments passed by
  reference (`&entity.title`). The guessed input of a creation makes those
  fields optional, and `DateTime` fields too (now when absent). The storage
  exposes `apply_<entity>_defaults(&mut entity, &given_fields)`.
- `stamps {}`: an `Option<DateTime>` (or `Option<Date>`) set to now when the
  row starts matching the condition, kept while it does, cleared when it
  stops — after every insert or update, in SQL.

#### Shorthand forms

The full block above can be collapsed into a single line. Two equivalent
shorthand spellings exist for `psql`:

```tr
infra: storage psql TaskInfra { entity: Task }

infra: storage:psql TaskInfra { entity: Task }
```

Both are parser-recognized and generate identical code to the block form.
There is no shorthand for `grpc_client` — it must use the full block form.

### `grpc_client`
gRPC client storage backend for cross-binary calls. Implements the same
generic `Storage<T, E>` trait shape as `psql`, so downstream `call:`/`errors {}`
syntax is unchanged regardless of backend.

> **Limitation:** only `find_one` is currently implemented as a mappable
> storage method for `grpc_client`. Mapping any other operation
> (`save_one`, `list`, `update_one`, `delete`, etc.) in `methods { ... }`
> is accepted by the parser but not supported by codegen yet. See
> [limitations.md](limitations.md).

### `service`
Inside `grpc_client`, points to the target gRPC adapter service.

### `methods`
Inside `grpc_client`, maps storage operations to RPC names.

```tr
infra RemoteTaskInfra {
    storage {
        grpc_client {
            service: GrpcWorkerAdapter
            on: "127.0.0.1:15051"
            entity: Task
            methods {
                find_one: GetTask
            }
        }
    }
}
```

### `s3`
Object-storage backend (MinIO, or any other S3-compatible service) for blob
data — photos, videos, arbitrary files.

```tr
infra MediaInfra {
    storage {
        s3 {
            bucket: "media"
            entity: MediaObject
        }
    }
}
```

**Does not implement the `Storage<T, E>` trait** `psql`/`grpc_client` share —
objects aren't rows. A `use_case` binds to it with [`blob`](#blob), not
`call`. `entity:` is **metadata only** today (e.g. `key`, `content_type`) —
the object's bytes never pass through a Trust-generated struct; nothing in
the entity's own fields is read or validated by the generated storage code
in this iteration. No shorthand form, same as `grpc_client`.

`bucket` is a plain string literal — it's not a secret, so it's fine in
source. Endpoint, access key, and secret key are **never** written in `.tr`
source at all (not even as `${VAR}` — there's no field for them in the DSL).
They're always read from three env vars derived from the infra name at
runtime, the same "credentials are environment-only" precedent `psql`'s
connection string already sets:

| Env var | Purpose |
|---|---|
| `<INFRA_NAME>_S3_ENDPOINT` | e.g. `http://localhost:9000` for local MinIO |
| `<INFRA_NAME>_S3_ACCESS_KEY` | — |
| `<INFRA_NAME>_S3_SECRET_KEY` | — |

In `--dev` mode these are scaffolded into `.env` with local-MinIO-friendly
defaults (`http://localhost:9000` / `minioadmin` / `minioadmin`) — see
[cli.md](cli.md#dev-mode---dev).

See [conversions.md §3b](conversions.md#3b-storage-trait-shape-s3) for the
exact generated shape, and [limitations.md](limitations.md) for what's not
supported yet (gRPC exposure, non-fixed input/output, `errors {}`
customization, operations beyond the three below).

### `http_client`
An outbound JSON HTTP client to another service — e.g. a payment API —
usable from [`code`](#code) use cases:

```tr
infra PaymentApi {
    http_client {
        base_url: "${PAYMENT_API_URL}"
        headers {
            "X-API-Key": "${PAYMENT_API_KEY}"
        }
    }
}
```

Generates `src/infrastructures/http_client/<infra>.rs` with a
`{Infra}Client` (here `PaymentApiClient`) exposing
`get_json`, `post_json`, `put_json`, `patch_json` and `delete_json` — each
takes a path appended to `base_url` (an empty path is `base_url` itself, a
full `https://...` URL is used as is), sends the configured headers, and
returns `Result<T, String>` (non-2xx statuses and undecodable bodies are
errors, with the response body in the message) — plus `get_text` (any text
body: an ICS feed, HTML, CSV), `post_form` (`x-www-form-urlencoded`, e.g.
OAuth token refresh), and `request(method, path)` / `send_raw(builder)` for
anything else (per-request headers, bearer tokens).

`get_json_paged(path, "items", "nextPageToken")` follows a paginated list
(the token goes back as the query parameter named after the key without
`next`: `pageToken`; `get_json_paged_with` names it) and returns every item.
`is_configured()` is false when `base_url` (or an `oauth2` value) resolved
empty.

`oauth2 { token_url client_id client_secret refresh_token }` (values
support `${VAR}`) puts a bearer token on every request: obtained from the
refresh token, cached, renewed a minute before it expires and after a 401
(the request is then retried once). `access_token()` returns it.

```tr
infra GoogleCalendar { http_client {
    base_url: "https://www.googleapis.com/calendar/v3"
    oauth2 {
        token_url: "https://oauth2.googleapis.com/token"
        client_id: "${GOOGLE_CLIENT_ID}" client_secret: "${GOOGLE_CLIENT_SECRET}"
        refresh_token: "${GOOGLE_REFRESH_TOKEN}"
    }
} }
```

Optional settings: `retry: 3` retries a network error, a 429 or a 5xx up to
3 times (backoff 200 ms, 400 ms, ...); `timeout: 10` gives up after 10 s
(30 by default). `base_url` and header values
support `${VAR}`, resolved when the binary starts, so secrets never live in
`.tr` source; `--dev` scaffolds each variable as `changeme`. Pulls in
`reqwest` (`json`) and `serde_json`.

### `ics_feed`
An `http_client` reading a published iCalendar feed (an Outlook or Google
"secret address"):

```tr
infra OutlookIcs { ics_feed { url: "${OUTLOOK_ICS_URL}" timezone_fallback: "Europe/Paris" all_day: skip } }
```

`deps.outlook_ics().occurrences(from, to)` returns the
`IcsOccurrence { uid, instance_id, title, start, end, location, description, all_day }`
overlapping `[from, to)`, sorted by start: RRULEs expanded (EXDATE
honoured, a RECURRENCE-ID replaces its occurrence, cancelled events
dropped), `instance_id` = the UID, plus `#<start in UTC>` for an
occurrence of a recurring event. TZIDs that are IANA names are used; common
Windows names (Outlook's "Romance Standard Time") are mapped; others, and
floating times, read in `timezone_fallback` (UTC by default).
`all_day: include` keeps all-day events as local-midnight spans (skipped by
default). `occurrences_with_warnings` also returns what couldn't be read.
Also takes `retry` and `timeout`. Pulls in `rrule`.

### `mirror`
A use case copying a source into a psql infra over a time window:

```tr
mirror OutlookMirror {
    source: OutlookIcs                       // an ics_feed infra, or a use case
    into: EventInfra
    key: [user_id, kind, external_id]        // becomes the infra's upsert key
    scope { kind: "outlook"  end_at > window.from  start_at < window.to }
    map { kind: "outlook"  external_id: item.instance_id  title: item.title
          start_at: item.start  end_at: item.end  domain: "pro"  note: item.location }
    window { from: week_start  days: 28 }    // now | today | week_start [("Europe/Paris")]
}
```

generates `OutlookMirrorUseCase` (input `OutlookMirrorRequest { days:
Option<Int> }` overriding the window length, output
`MirrorReport { configured, written, removed, error }`), usable in routes,
tools and `schedule`s. It reads the source over `[from, from + days)`,
upserts each item on `key` (the owner is the running user), then deletes
the owner's rows matching `scope` whose key the source no longer has. A
failure is reported in `error` (nothing is deleted); an `ics_feed` whose URL
is empty reports `configured: false`. `today` / `week_start` use the
`config { tz }` zone unless one is given.

A use case source receives `{ from, to }` (its input type's fields of
those names) and its output's `items` list is mirrored — e.g. a `code` use
case paging through an API with `get_json_paged`. `map {}` values are
`item.<field>` (a value goes into an `Option` field as `Some`) or literals.

### `bucket`
Inside `s3`, the literal bucket name (a plain string, not env-driven —
see [`s3`](#s3) above for why this differs from endpoint/credentials).

## Use Case Keywords

### `use_case`
Declares business logic entry points.

### `input`
Sets input type. Shared by **every** `call` on the same use case — you
can't give different calls different input shapes. If omitted, it's
inferred from the `call` chain (e.g. from the target storage entity/method).
This is the **authoritative** input shape — see the note on route/`rpc`
`input:` below, which is not the same thing.

### `output`
Sets output type. Determined by the **last** declared `call` if multiple
calls are present (see [`call`](#call) below), or inferred if omitted.
Authoritative for the same reason as `input`.

On a `call:` use case, `output:` may also name a type keeping some of the
entity's fields (same types) — and, with `journal:`, an `action_id`
(`Int` / `Option<Int>`) filled with the entry written:

```tr
type SaveTaskResult = Omit<Task, user_id> + { action_id: Option<Int> }
use_case SaveTaskUseCase { call: TaskInfra.save_or_patch output: SaveTaskResult journal: Actions }
```

The response is then that type, flat, instead of `{ "task": { ... } }`.

### `call`
Binds use case logic to infra operations.

```tr
use_case GetTaskUseCase {
    input: GetTaskRequest
    output: Task
    call: TaskInfra.find_one
}
```

**Multiple `call:` lines are allowed** and have specific semantics — this is
easy to miss:

```tr
use_case ArchiveTaskUseCase {
    call: AuditLogInfra.save_one
    call: TaskInfra.update_one
}
```

- Calls run in declaration order; only the **last** one shapes the generated
  `{Name}Output` and drives `NotFound`/list-vs-single semantics. Earlier
  calls act as shared side effects/guards using the same input.
- The same infra target appearing in more than one `call` is deduplicated to
  one struct field/generic — you don't get two handles to it.
- Each `call` keeps its own independent `errors {}` mapping, even when two
  calls share an infra target.
- Only calls bound to `save_one`/`update_one` generate a `From<Input> for
  Entity` conversion (reads/deletes/lists don't need one).

See [conversions.md](conversions.md#5-use_case-multi-call-semantics) for the
full breakdown.

### `filters`
Scopes a `list`-bound primary `call:`'s results. One predicate per line:
`<field> <operator> <value>`.

```tr
use_case ListOpenTasksUseCase {
    call: TaskInfra.list
    owner: user_id
    filters {
        status in ["todo", "doing"]
        project_id: input.project_id
        priority >= input.min_priority
        title contains input.q
        due < now()
        day >= today("Europe/Paris")
    }
    order_by: priority desc, due
}
```

| Operator | Meaning |
|---|---|
| `:` / `=` / `==` | equal |
| `!=` | not equal |
| `>` `>=` `<` `<=` | comparisons (numbers, dates, text) |
| `in` / `not in` | one of a `[list]` or of `input.<field>` (a comma-separated string, or a `List`) |
| `like` / `ilike` | SQL pattern (`%`, `_`), case-sensitive / not |
| `contains` | case-insensitive substring |

Values: `input.<field>`, a literal (quoted string or bare word: `true`,
`42`), a `[list]` for `in`, `now()` (UTC time of the request), `today()` or
`today("Europe/Paris")` (the current date, in UTC or in that zone).

- Only valid when the **primary** (last) `call:` is `list`; the field must
  exist on its entity and can't be a JSONB list/struct column.
- With no explicit `input`, every `input.<field>` is added to the guessed
  input with the entity field's type (`String` for `in`/patterns).
- An `input.<field>` whose type is `Option<T>` filters only when it is set
  (`?project_id=3` filters, no parameter lists everything).
- Values are sent as text and the *parameter* is cast to the column type,
  so indexes stay usable. Trust creates a btree index on the equality
  columns followed by the first sort/range column.
- A path parameter reaches a filter field by name like any input field.

### `order_by`
`order_by: field [asc|desc], ...` sorts a `list` primary call (`id` always
breaks ties, so pages are stable). Default: by `id`.

### `limit` / `offset`
Every guessed `list` input has optional `limit` (100 when absent, 1000 at
most) and `offset` (0). An explicit input may declare them (`Int` or
`Option<Int>`) or leave them out.

### `errors`
Maps infra/storage errors into use case errors.

```tr
call: {
    name: TaskInfra.find_one
    errors {
        DatabaseError -> InternalError
        ConversionError -> BadRequest("invalid task data")
    }
}
```

Notes:
- Storage errors always have exactly two source variants to map from:
  `DatabaseError` and `ConversionError` (fixed regardless of backend).
- The literal message argument (`BadRequest("...")`) is optional — a bare
  target name (`InternalError`) is also valid. With a literal message, the
  real underlying error is discarded and the literal is always used; without
  one, the real error string is threaded through into your target variant.
- Each source variant may be mapped at most once per `call`; mapping the
  same source twice is a parse-time error.
- Mapping is **partial, not all-or-nothing**: any source variant you don't
  mention still falls back to the use case's built-in `StorageError(String)`
  variant.
- `NotFound` and `StorageError` always exist on the generated `{Name}Error`
  enum regardless of what you map, and (see [`status`](#status) below) their
  eventual HTTP status codes are hardcoded and **not** overridable.

### `patch_one`, `upsert_one` and `save_or_patch`
More `call:` verbs on `psql` infras:

- `patch_one` updates only the fields the request carries. Its guessed
  input is `id` plus every other field as `Option`; a missing field is left
  as is and, for a nullable column, `null` clears it. Missing row → 404.
- `upsert_one` inserts the entity, or updates the row with the same value
  of the infra's first `unique` key (a codegen error without one).

- `save_or_patch` creates without `id` (from the fields given and the
  infra's `defaults {}`; a missing field without default is a
  `MissingField` error, 422) and patches with one (`null` clears).

All honour `owner:` (an upsert never takes over someone else's row; it
answers 404 instead). Storages also offer `get_or_default(owner_field,
owner)`: the owner's row, or a new unsaved one from the `defaults {}`.

### `transaction`
`transaction: true` runs every `call:` of a use case — or the whole body of a
`code {}` use case — in one Postgres transaction: committed when the run
succeeds, rolled back on any error. Every infra involved must be `psql`, on
the same database. A `code {}` body can also manage one itself:
`let tx = self.task_infra.begin().await?;`, `self.task_infra.in_transaction(&tx)`
returns a storage bound to it, then `crate::commit_transaction(&tx)` /
`crate::rollback_transaction(&tx)`.

### `notify`
`notify: true|false` — whether a successful run is announced on HTTP
`events:` streams. Default: true when a call writes (`save_one`,
`update_one`, `patch_one`, `upsert_one`, `delete`, ...), false otherwise
(reads, `code {}`).

### `owner`
Scopes a `call`-based use case to **the calling user** — the `user_id` of
the route's JWT (see [`auth`](#auth)). Names an `Int` field of the primary
(last) call's entity:

```tr
use_case ListMyCartUseCase {
    call: CartInfra.list
    owner: user_id
}
```

What the primary call does with it:

| Call | Behavior |
|---|---|
| `save_one` | stamps `entity.<owner> = user` before saving |
| `list` | adds a `<owner> = user` predicate (combines with `filters`) |
| `find_one` | a row owned by someone else is `NotFound` |
| `update_one` / `delete` | one statement filtered on `id` **and** the owner (`... WHERE id = $n AND <owner> = $m`); not owned → `NotFound` (nothing is written); `update_one` also re-stamps the owner |

- The owner field is **never part of a guessed input**, so clients can't
  choose it. An explicit input that has it is simply overwritten.
- Someone else's row reads as `404`, never `403`, so ids can't be probed.
- Every HTTP route exposing an `owner` use case **must declare `auth`**
  (either form) — codegen error otherwise. gRPC can't expose them yet.
- Codegen errors: unknown field, or a field that isn't `Int`.

### `code`
A use case whose `execute` body is **hand-written Rust**, read from a file,
for logic CRUD can't express (checkout, totals, calls to other services,
webhooks). Everything around the body — error enum, trait, struct,
constructor, `main.rs` wiring — is generated as usual. Mutually exclusive
with `call`, `ai` and `blob`.

```tr
use_case CheckoutUseCase {
    input: CheckoutRequest
    output: CheckoutResponse
    code {
        file: "./logic/checkout.rs"
        infra: CartInfra
        infra: OrderInfra
        infra: PaymentApi
        errors { EmptyCart PaymentUnavailable }
        deps { uuid: "1" }
    }
}
```

- `file` (required): path relative to the working directory. Its content
  is pasted as the body of
  `async fn execute(&self, user: i64, input: Input) -> Result<Output, {Name}Error>`
  — `user` is the JWT user id on routes with `auth`, `0` otherwise.
- `input`/`output` are required (declared types, or the built-in
  [`RawRequest`](#rawrequest) for the input).
- `infra:` (any number): each becomes a field named after the infra in
  snake_case — `self.cart_infra`, `self.payment_api`. `psql`/`grpc_client`
  infras are generic storage handles (`list`, `save_one`, `find_one`, ...
  from `common::Storage`); `s3` infras are the concrete `S3{Entity}Storage`;
  [`http_client`](#http_client) infras are the concrete `{Infra}Client`.
- `errors {}`: extra `Variant(String)` members of `{Name}Error`, next to the
  always-present `NotFound` and `StorageError(String)`. Routes map them
  with [`status`](#status) like any other error target.
- `deps {}`: extra crates (`name: "version"`, or `"chrono-tz": "0.10"` for
  names with a dash) for the generated `Cargo.toml`.
- `infra: [A, B]` lists several infras at once, and `infra_set DayData { A B
  C }` (top level) names a group usable as `infra: DayData`.
- `use_case: PlanWeekUseCase` (repeatable): another use case the body runs,
  `self.plan_week_use_case.execute(user, input).await` — `?` converts its
  error (`NotFound` stays `NotFound`). Use cases are built in dependency
  order; a cycle is an error.
- `module: "./capture.rs"` instead of `file:` makes the file a real Rust
  module: it defines `pub async fn execute(deps: &impl CaptureUseCaseDeps,
  user: i64, input: In) -> Result<Out, CaptureUseCaseError>` (plus helpers
  and `#[cfg(test)]` tests, run by `cargo test` in the generated crate) and
  reaches its infras through `deps.inbox_infra()`, `deps.plan_use_case()`.
  Start it with `use crate::*;`.
- `include: "./logic/shared.rs"` (repeatable): a Rust file copied once into
  the crate (`src/shared/`); its `pub` items are visible from every `code {}`
  body — for logic several use cases share (a priority score, a parser).
- `?` works on every infra: storage errors and the `String` errors of
  `s3`/`http_client` convert into `StorageError`.
- Storages also offer `query(Query::new().eq("user_id", user).gt("end_at",
  from).lt("start_at", to).order_by("start_at"))` (`eq ne gt gte lt lte like
  ilike contains is_in not_in order_by order_by_desc limit offset`; 1000 rows
  by default) and `find_one_owned(id, "user_id", user)`.
- The body (and every `include:` file) starts with a single glob, `use
  crate::*;`: every generated type plus `common::{Storage, StorageListQuery,
  StoragePredicate, StoragePredicateOperator, UseCase}`. Being a glob, it
  never clashes with your own `use` lines. Apps containing a `code` use case keep **all** declared
  types, since the body may name any of them.

### `RawRequest`
Built-in input type for `code` use cases that need the request exactly as
received — typically a webhook verifying an HMAC over the raw body:

```rust
pub struct RawRequest {
    pub body: String,
    pub headers: HashMap<String, String>, // lowercase names
}
```

Used as `input: RawRequest` (when no type of that name is declared). The
route handler passes the untouched body and headers instead of decoding
JSON. HTTP routes only, and such a route can't have path parameters.

### `ai`
Switches use case implementation to AI-authored function body generation.
Mutually exclusive with `call`, `blob` and `code` — a use case declaring
more than one is a hard error (`cannot declare more than one of 'call',
'ai', 'blob', 'code' — they're mutually exclusive`).

### `prompt`
Prompt text used by `ai` generation.

```tr
use_case ValidateEmailUseCase {
    input: ValidateEmailRequest
    output: ValidateEmailResponse
    ai {
        prompt: 'Return Ok if email is valid, otherwise an error.'
    }
}
```

An `ai` block may optionally carry an `infra:` reference, giving the
AI-authored function access to a storage infra the same way a `call` would:

```tr
use_case SummarizeDocUseCase {
    ai {
        infra: DocInfra
        prompt: 'Summarize the document fetched from infra in two sentences.'
    }
}
```

The AI call happens **once, at compile time** (a real HTTP call to the
Anthropic Messages API from inside the `trust` binary itself, using
`--ai-api-key`/`ANTHROPIC_API_KEY`); the returned code is spliced directly
into the generated function body. The generated project itself gets **no**
extra runtime dependency and never talks to any AI provider at runtime. An
AI use case's error enum is always exactly `{Name}Error::{NotFound,
StorageError(String)}` — `errors {}` customization isn't available for `ai`
use cases.

### `blob`
Binds a use case directly to one [`s3`](#s3) infra operation. Mutually
exclusive with `call` and `ai`.

```tr
use_case RequestUploadUrlUseCase {
    blob: MediaInfra.presigned_put_url
}
```

- `Target` must name an `s3 {}` infra (not `psql`/`grpc_client`) — a hard
  codegen error otherwise (`'blob' references '...', which isn't an s3 {}
  infra`).
- `operation` is one of exactly three values, validated at codegen time (not
  parse time — same pattern as `grpc_client`'s `methods {}` mapping names):
  `presigned_put_url`, `presigned_get_url`, `delete`. Anything else is a
  clear error (`Unsupported blob operation '...'`).
- **Input/output are always a fixed, non-overridable shape** — there's no
  entity-driven guessing the way CRUD `call:` has. Every operation takes
  `{ key: String }`; `presigned_put_url`/`presigned_get_url` return
  `{ url: String }`, `delete` echoes `{ key: String }` back. Declaring an
  explicit `input`/`output` on a `blob {}` use case is a hard error
  (`'blob' use cases have a fixed input/output shape and cannot declare
  their own`) — unlike a route/`rpc`'s `input`/`output`, there's nothing
  here for one to narrow or cross-check.
- Same fixed error enum as `ai {}`: `{Name}Error::{NotFound,
  StorageError(String)}`, no `errors {}` customization. `NotFound` is never
  actually constructed by a blob operation today — it stays part of the
  shape purely so the adapter's status-mapping match stays exhaustive
  regardless of use case kind.
- The generated use case struct holds a **concrete** `S3{Entity}Storage`
  field, not a generic `S0: Storage<T, E>` bound the way CRUD use cases
  do — blob storage doesn't implement that trait. This is transparent to
  routing: the HTTP/gRPC adapter's own genericity (`{Name}Trait`) still
  applies the same way regardless.

## Adapter Keywords

### `adapter`
Declares an adapter boundary.

### `http`
Declares HTTP adapter configuration.

### `cors`
Browser origins allowed to call an HTTP adapter (comma-separated, or `*`
for any), resolved at runtime with the same `${VAR}` interpolation as
`on:`:

```tr
adapter HttpApi {
    http {
        on: "${HOST}:${PORT}"
        cors: "${CORS_ORIGINS}"
        routes { ... }
    }
}
```

Adds a `tower-http` `CorsLayer` (any method, any header) to the adapter's
router. Without `cors:` no CORS headers are sent — browsers on another
origin can't call the API. `--dev` scaffolds `*CORS*`/`*ORIGIN*` variables
as `http://localhost:5173` (Vite's dev server).

### `routes`
Route list for HTTP adapter.

> **A route's `input`/`output` don't define the generated shape — the
> use_case's own `input`/`output` does.** If the referenced use case has no
> explicit signature of its own, whatever you write here is silently
> ignored. If it does, what you write here must exactly match it, or it's a
> hard codegen error. Treat route-level `input`/`output` as an optional,
> self-documenting contract check, not the source of truth. Full detail:
> [conversions.md §10](conversions.md#10-routerpc-inputoutput-are-a-cross-check-not-the-source-of-truth).

### `status`
Route-level error-to-HTTP-status mapping.

```tr
status {
    InternalError -> 500
    BadRequest -> 400
    _ -> 502
}
```

- `NotFound` and `StorageError` are **hardcoded** to `404` and `500`
  respectively and **cannot be overridden** by a `status {}` entry — the
  generated match always emits those two arms unconditionally, regardless of
  what you write here.
- For every other (custom) error target: an exact `Target -> N` entry wins;
  otherwise the route's own `_ -> N` wildcard (if present) is used;
  otherwise it falls back to `500`.
- gRPC `rpc` methods have **no equivalent** — there is no way to customize
  gRPC error-to-status mapping at all (see [`rpc`](#rpc) below).

### `body`
Used in wrapped input style (`input { body: Type }`).

> **HTTP-only.** The `input { body: Type }` block form is only accepted on
> HTTP routes. A gRPC `rpc` requires the bare `input: Type` colon form —
> using the block form there is a parse error. Both forms generate
> identical code where they are both valid; this is purely a syntax
> restriction on `rpc`.

Path-param fields (`:id` in the route) and body fields can be mixed freely
in one input type — Trust automatically splits them into a path extractor
and a JSON body extractor behind the scenes, then reassembles them before
calling your use case. See [conversions.md](conversions.md#6-http-path-parameter-conversion).

### `auth`
Route-level authorization requirement, in two forms. Both extract the
caller's JWT (`auth::UserContext`, **401** when missing or invalid) and
pass its `user_id` to the use case's `execute` — which is what
[`owner`](#owner) and [`code`](#code) bodies rely on. Routes without
`auth` call use cases as user `0`.

**Login only** — any valid JWT:

```tr
GET "/cart" {
    use_case: ListMyCartUseCase
    auth: required
}
```

**Shared secret** — for machines (webhooks, cron, an agent), not people:

```tr
POST "/webhooks/telegram" {
    use_case: TelegramWebhookUseCase
    auth { secret: "${TELEGRAM_SECRET}" header: "X-Telegram-Bot-Api-Secret-Token" user: 1 }
}
```

The request must carry the secret (compared in constant time) in `header`,
or as `Authorization: Bearer <secret>` without one; otherwise 401. The use
case runs as `user` (0 by default). No JWT is involved.

**Permission row** — additionally requires a matching grant:

```tr
DELETE "/tasks/:id" {
    use_case: DeleteTaskUseCase
    auth {
        infra: PermissionInfra
        target: "task"
        action: "delete"
    }
    status {
        _ -> 500
    }
}
```

This is a **permission-row lookup gated behind JWT identity**, not a bare
JWT-claims check. At request time the generated handler:

1. Extracts a `UserContext` from the request (JWT decoding, via the external
   `auth` crate — this logic isn't regenerated per project).
2. Queries `infra:`'s storage for a row matching `user_id == <from JWT>`,
   `target == "<target>"`, `action == "<action>"`.
3. If no matching row is found, the request short-circuits with
   **403 Forbidden**. A storage failure during the lookup itself maps to
   **500**.

`infra:` should point at a storage infra whose entity models permission
grants (see `mod/user_permission` for a reference shape). Any HTTP adapter
with at least one `auth`-requiring route pulls in the `auth` crate as a
dependency, and (in `--dev` mode) gets a scaffolded `JWT_SECRET=changeme`
line in its `.env` — `auth::UserContext` reads that variable to validate
JWTs at runtime.

### HTTP adapter options

```tr
adapter Api {
    http {
        on: "0.0.0.0:${PORT}"
        cors: "${CORS_ORIGINS}"
        envelope: false
        events: "/events" auth: required
        routes { ... }
    }
}
```

- `envelope: false` — responses are the entity or list itself (`{...}`,
  `[...]`) instead of `{"task": {...}}` / `{"tasks": [...]}`.
- `events: "/path"` — a Server-Sent Events stream: one `change` event
  (`{"use_case", "user", "at"}`) per successful run of a use case with
  `notify`. With `auth: required`, the JWT comes from the `Authorization`
  header or an `access_token` query parameter (a browser `EventSource`
  can't set headers) and only the caller's own events are sent.
- Non-GET routes read a JSON body; an empty body reads as `{}` (inputs with
  no required field need no body). Malformed JSON is a 400, a body of the
  wrong shape a 422.
- Every HTTP adapter answers `GET /health` (`{"status":"ok"}`) unless a route
  uses that path, and logs each request.

### `journal`
An undoable log of what use cases write:

```tr
type Action {
    id: Int  user_id: Int  kind: String  text: String  changes: Json
    undone: Bool  at: DateTime  requires_ack: Bool
}
infra ActionInfra { storage { psql { entity: Action owner: user_id } } }

journal Actions { infra: ActionInfra owner: user_id undo: "/actions/:id/toggle" conflict: refuse }

use_case AcceptInboxItemUseCase { ... journal: Actions text: "« {input.title} » accepted (task {output.task_id})" }
resource Tasks { ... journal: Actions }
adapter Api { http { ... resources { Tasks Actions } } }
```

- The journal's entity needs `id`, the owner field (`Int`), `kind: String`,
  `text: String`, `changes: Json`, `undone: Bool` and `at: DateTime`; other
  fields take their default (or a value set from code, below).
- A use case with `journal:` (and a resource's writes) runs in one
  transaction (psql infras on one database) and records every row it writes.
  On success it writes an entry: `kind` = the use case name, `text` = its
  `text:` (`{input.x}`, `{output.a.b}` placeholders; the use case name by
  default), `changes` = `[{ "table", "id", "before", "after" }]` — table
  names and `to_jsonb` rows with column names, `null` for a creation /
  deletion. A run that changed nothing writes no entry.
- An output field `action_id: Option<Int>` (or `Int`) receives the entry's id.
- From a `code {}` body: `self.trust_journal` (or `deps.journal()` in a
  `module:` file) — `.entry("text").requires_ack(true)` / `.set("field",
  value)` starts a new entry for the writes that follow; `.text("...")`
  replaces the current entry's text.
- `undo: "/path/:id"` generates `Toggle{Journal}UseCase` (input `{ id }`,
  output the entry) and, in adapters listing the journal under
  `resources {}`, a `POST` route: an entry not undone is undone (every row
  back to `before`), an undone one redone. With `conflict: refuse` a row
  changed since answers 409; `overwrite` skips the check. Another user's
  entry is a 404.

### `resource`
CRUD use cases and routes in one block:

```tr
resource Tasks {
    infra: TaskInfra
    path: "/tasks"
    owner: user_id
    list { order_by: priority asc, due asc  filters { status: input.status } }
    create
    get
    patch
    delete
}

adapter Api { http { on: "..." auth: required resources { Tasks } routes { ... } } }
```

| Operation | Use case | Route |
|---|---|---|
| `list` | `List{Resource}UseCase` (`list`) | `GET /tasks` |
| `create` | `Create{Entity}UseCase` (`save_one`) | `POST /tasks` |
| `upsert` | `Upsert{Entity}UseCase` (`upsert_one`) | `PUT /tasks` |
| `get` | `Get{Entity}UseCase` (`find_one`) | `GET /tasks/:id` |
| `update` | `Update{Entity}UseCase` (`update_one`) | `PUT /tasks/:id` |
| `patch` | `Patch{Entity}UseCase` (`patch_one`) | `PATCH /tasks/:id` |
| `delete` | `Delete{Entity}UseCase` (`delete`) | `DELETE /tasks/:id` |
| `save` | `Save{Entity}UseCase` (`save_or_patch`) | `PUT /tasks` |

`singleton` (with `owner`) makes a one-row-per-user resource: `get` answers
`GET /settings` with the caller's row or one built from the defaults
(`default { day_start: "08:00:00" }` adds to the infra's `defaults {}`),
`upsert` writes it (`PUT /settings`); the infra gets `unique: [owner]`.

The use cases are ordinary ones: `mcp { tools }` and `schedule` can name
them. `name: Event` sets the singular of generated names
(`CreateEventUseCase`) when it differs from the entity's name. Routes appear in every HTTP adapter listing the resource under
`resources {}`, with that adapter's default `auth`.

### Derived types
Structs computed from another one, with optional extra fields:

```tr
type NewTask = Omit<Task, id, user_id, done_at>
type TaskDraft = Partial<Omit<Task, user_id>> + { clear: Option<List<String>> }
type TaskWithAction = Task + { action_id: Option<Int> }
```

`Omit<T, a, b>` drops fields, `Pick<T, a, b>` keeps only those,
`Partial<T>` makes every field `Option`; they nest. Naming a field the base
doesn't have is an error.

### `database`
One Postgres database, and one connection pool, shared by the `psql` infras
naming it:

```tr
database Main { url: "${DATABASE_URL}" pool: 16 }
infra TaskInfra { storage { psql { entity: Task database: Main } } }
```

Without `database:`, an infra reads `<INFRA>_DB_URL`, falling back to
`DATABASE_URL` (pool: `<INFRA>_DB_POOL_SIZE`, `DATABASE_POOL_SIZE`, 16).
Infras resolving to the same URL share one pool either way, so one
`DATABASE_URL` is enough for the whole app.

### Defaults
- **Adapter `auth`**: `auth: required` (or any `auth` form) inside `http {}`
  applies to every route that doesn't write its own; a route opts out with
  `auth: none`.
- **Infra `owner`**: `psql { owner: user_id }` is the `owner:` of every `call:`
  use case whose primary call targets the infra; a use case opts out with
  `owner: none`.
- **Environment variables**: `${NAME:-default}` anywhere `${NAME}` works.
  Variables of `http_client {}` and `auth { secret }` without a default read
  as empty when unset (the client fails when used, the secret never
  matches) instead of stopping the program. The binary loads `.env` from the
  working directory at startup.
- **Create inputs**: the guessed input of `save_one` / `upsert_one` has neither
  `id` nor the owner field.

### `config`
Settings for the whole file:

```tr
config { json_case: "camel" }
```

`deps { "chrono-tz": "0.10" }` adds crates to every app (for code shared by
several `code {}` bodies).

`json_case: "camel"` renames every generated JSON field to camelCase
(`weekly_budget_h` → `weeklyBudgetH`) in requests, responses, query strings
and JSONB columns. Path parameters keep the names written in the route.

`tz: "Europe/Paris"` is the app's time zone: `today()` in `filters {}` and
psql `defaults {}`, and `schedule`s without their own `tz`, use it. Code
gets `crate::tz::*`:

| Helper | |
|---|---|
| `now_local()`, `today()` | now / today in the zone |
| `local(t)`, `local_date(t)`, `local_time(t)` | an instant seen in the zone |
| `local_to_utc(naive)`, `at(date, time)`, `midnight(date)` | a wall-clock time as an instant — an ambiguous time (clocks back) is its first occurrence, a time in the gap (clocks forward) keeps the offset before it (02:30 → 03:30) |
| `day_bounds(date)`, `week_bounds(t)` | `[start, end)` instants of a local day / Monday-based week |
| `week_start(t)` | the Monday of `t`'s local week, as a date (`week_bounds(t).0` is its 00:00 instant) |

`locale: "fr"` (or `"en"`) adds `crate::fmt::*`, in the app's zone (UTC
without `tz`), taking a `DateTime<Utc>`, a `NaiveDateTime` or a `NaiveDate`:

- `fmt_date(t, "EEEE d MMMM")` → "mardi 10 novembre"; `EEE dd/MM H:mm` →
  "mar. 10/11 14:30" (`EEEE`/`EEE` weekday, `d`/`dd`, `M`/`MM`/`MMM`/`MMMM`,
  `yy`/`yyyy`, `H`/`HH`, `mm`, `ss`, `'literal'`);
- `fmt_time(t)` → "14h30", "9h" (en: "14:30");
- `fmt_relative_day(t)` → "aujourd'hui", "demain", the weekday within the
  week, "en retard" before today, else the date; `fmt_relative_day_from(t,
  now)` relative to a given day instead of the clock;
- `plural(n, "réunion", "réunions")` → "1 réunion", "3 réunions" (0 and 1
  are singular in French).

### `map`
A conversion between two types:

```tr
map InboxItem -> Task {
    title: title
    domain: suggestion.domain        // nested fields; through an Option too
    due: suggestion.due
    source: source                   // T into an Option<T> field: Some(...)
    note: "from the inbox"           // literals
}
```

generates `Task::from_inbox_item(&item)`, `item.to_task()` and
`From<&InboxItem> for Task`. Unlisted fields take the target's psql
`defaults {}`, else their type's default. Types must match; an optional
value can't fill a required field.

### `schedule`
Runs use cases on a timer, inside the app that lists the schedule in its
`run {}` block:

```tr
schedule Jobs {
    tz: "Europe/Paris"
    user: 1
    "*/15 * * * *" => SyncCalendarsUseCase
    "30 7 * * *" => SendBriefUseCase
    "0 18 * * sun" => PlanWeekUseCase tz: "UTC"
}

app Main: run { Api Jobs }
```

- Five-field cron (`minute hour day-of-month month day-of-week`) with `*`,
  `a-b`, `*/n`, lists and `jan`/`mon` names; checked by Trust at compile
  time. When both day fields are restricted, either one matches.
- `tz` (IANA name, UTC by default) applies per schedule or per line;
  daylight-saving changes are handled.
- Each run calls `execute(user, input)` as `user` (0 by default), with the
  input type's defaults — or with literal fields over them:
  `"30 7 * * *" => SendBriefUseCase { send: true label: "morning" }`
  (field names are checked at compile time). A run never overlaps the next one of
  the same line. Results and errors are logged.

### `mcp`
A Model Context Protocol server: each tool runs one use case, with an input
schema derived from the use case's input type.

```tr
adapter Agent {
    mcp {
        on: "127.0.0.1:${MCP_PORT}"
        auth { secret: "${MCP_TOKEN}" user: 1 }
        instructions: "My World tools: capture ideas, read the day."
        tools {
            capture { use_case: CaptureUseCase description: "Add an item to the inbox" }
            today { use_case: TodayUseCase description: "Tasks and events of the day" }
        }
    }
}
```

- Transports: Streamable HTTP at `POST /mcp` (JSON responses), and stdio with
  `<binary> mcp` for clients that start the server themselves.
- `auth: required` (JWT, e.g. one from `<binary> token`) or `auth { secret
  ... }`; without auth, tools run as user 0 and `owner` use cases are
  refused.
- Tool results come back as text and `structuredContent`; a use case error
  is a result with `isError: true` and a readable text (`MissingField: title
  est obligatoire`, `NotFound`). `RawRequest` use cases can't be tools.
- `resources { goals { use_case: ListGoalsUseCase uri: "world://goals"
  description: "..." } }` — read-only context: `resources/read` runs the use
  case with its default input and returns its output as JSON.
- `oauth { public_url: "${MCP_PUBLIC_URL}" password: "${MCP_OAUTH_PASSWORD}"
  user: 1 }` — for remote clients such as claude.ai custom connectors: the
  server becomes an OAuth 2.1 protected resource *and* a single-user
  authorization server (`/.well-known/oauth-protected-resource`,
  `/.well-known/oauth-authorization-server`, dynamic registration at
  `/oauth/register`, a password page at `/oauth/authorize`, PKCE S256, refresh
  tokens at `/oauth/token`). A 401 carries `WWW-Authenticate: Bearer
  resource_metadata=...`. Access tokens are the usual JWTs (`JWT_SECRET`, 1
  h); refresh tokens last 30 days and are signed with a derived key, so they
  are not access tokens. `public_url` must be the URL clients reach (behind
  the reverse proxy).

### `grpc`
Declares gRPC adapter configuration.

### `services`
gRPC service RPC declaration block.

### `rpc`
Declares one gRPC method endpoint.

> Same rule as HTTP `routes`: `input:`/`output:` here are an optional
> cross-check against the referenced use_case's own signature, not the
> source of truth for the generated proto message shape — see
> [conversions.md §10](conversions.md#10-routerpc-inputoutput-are-a-cross-check-not-the-source-of-truth).

```tr
adapter InternalGrpcApi {
    grpc {
        services {
            rpc GetUser {
                input: GetUserRequest
                output: User
                use_case: GetUseCase
            }
        }
        on: "0.0.0.0:50051"
    }
}
```

> **Field type limitation, not currently in limitations.md at HTTP-parity
> visibility:** gRPC message fields only support `String` and `Int` (`i64`).
> `Uuid`, `Float`, `Decimal`, list-shaped (`Vec<...>`) outputs, and nested
> struct fields are a **generation-time error** ("gRPC doesn't support field
> type '...' yet — only String and Int fields are supported"). Design
> gRPC-exposed types with flat `String`/`Int` fields only.

Error mapping for `rpc` is **not customizable** — the generated match always
maps your error enum's `NotFound` variant to `Status::not_found(...)` and
every other variant to `Status::internal(...)`, with no `status {}` block
equivalent.

A CRUD-style use case's entity output is wrapped in a single-field "wrapper"
proto message (e.g. `message CreateUserOutput { User user = 1; }`) rather
than duplicating the entity's fields inline.

## App and Runtime Keywords

### `app`
Declares a runnable app unit. Any `.tr` file with at least one `app` block
compiles to a **Cargo workspace** in `./out` — even with just a single
`app` — with one crate per app. A file with **zero** `app` blocks falls back
to one flat, non-workspace crate.

### `run`
App runtime binding block.

```tr
app Main: run {
    HttpAdapter
    InternalGrpcApi
}
```

### `load_balancer`
Declares a round-robin reverse proxy app component. It never constructs any
adapter itself — it only proxies to independently-deployed, already-running
backend processes at the addresses in `targets`.

### `targets`
Load balancer backend address list. **These are always static string
literals** — `${VAR}` interpolation does **not** apply here, even though it
does apply to the load balancer's own `on:` and to adapter `on:` fields (see
[`on`](#on) below and [conversions.md](conversions.md#7-address-string--env-var-interpolation)).
Changing the backend fleet currently requires recompiling.

### `on`
Bind/listen address in adapters, `grpc_client` infra, and the load balancer
(its own listen address — not `targets`).

```tr
app Gateway: run {
    load_balancer ApiLb {
        targets {
            "10.0.0.1:8080"
            "10.0.0.2:8080"
        }
        on: "0.0.0.0:9000"
    }
}
```

Address strings on `on:` (adapters and the load balancer's own bind address)
accept `${VAR}` interpolation, resolved from the environment at runtime
rather than baked in as a fixed literal:

```tr
adapter HttpApi {
    http {
        on: "${HOST}:${PORT}"
        routes { ... }
    }
}
```

This generates a runtime lookup (`std::env::var("HOST")`,
`std::env::var("PORT")`) instead of a hardcoded address. In `--dev` mode,
any interpolated variable found this way is also written into that app's
generated `.env` file with a sniffed default (`PORT`-containing names share
a counter starting at 3000 and incrementing per variable within the app,
`HOST`-containing names default to `0.0.0.0`, everything else defaults to
`changeme`). Full detail: [cli.md](cli.md#dev-mode---dev).
