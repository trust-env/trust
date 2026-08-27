# Keywords

This page lists all language keywords currently recognized by Trust, what
each one does, and how to use it. For exact type/name conversion rules and
generated-code shapes, see [conversions.md](conversions.md); for full CLI
usage see [cli.md](cli.md).

## Syntax Notes

These apply across the whole language and aren't tied to one keyword:

- **No comment syntax.** There is no `#`, `//`, or `/* */` form. Any stray
  character that isn't part of a token (including `#`) is a hard lexer error.
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
  actually compile ("Undefined type reference: ...").

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
Has **zero** effect on the type's own generated code (same derives, same
visibility) — it only changes whether another file's `import` can find it.

```tr
type PublicUser {
    id: Int,
    name: String
    export PublicUser
}
```

### `import`
Imports exported items from another module.

```tr
import PublicUser from "./shared.tr"
import { User, Role } from "./shared.tr"
```

Only local path imports (`"./shared.tr"`) actually work end to end.
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

### `Regex`
Regex-annotated type form (`Regex<pattern>`).

```tr
type ApiKey {
    id: Int,
    label: Regex<slug>
}
```

**The pattern is purely documentation today** — `Regex<slug>` compiles down
to a plain `String` field, byte-identical to writing `String` directly. No
regex crate is added, no validation is generated or enforced anywhere. Also
note the pattern itself must be a single alphanumeric/`_` word — real regex
metacharacters (e.g. a literal `-`) break lexing.

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
Scopes a `list`-bound primary `call:`'s results by one or more equality
predicates, each comparing a storage field against one of the use case's
own input fields.

```tr
use_case ListRatingsForSpotUseCase {
    call: RatingInfra.list
    filters {
        spot_id: input.spot_id
    }
}
```

- Only valid when the **primary** (last) `call:` is bound to `list` — a
  hard codegen error otherwise (`'filters' is only valid when the primary
  call is 'list'`).
- The left-hand side (`spot_id`) must be a field on the primary call's
  entity — checked at codegen (`'filters' references unknown field '...'
  on entity '...'`).
- The right-hand side must be exactly `input.<field>` — no other
  expression form is accepted (no literals, no operators beyond implied
  equality). If you don't declare an explicit `input`, the referenced
  field is **added automatically** to the guessed list input (alongside
  `limit`/`offset`), with the same type as the matching entity field. If
  you do declare an explicit `input`, it must already have a field with
  that name, or it's a codegen error.
- Every `filters {}` entry becomes exactly one **equality** predicate
  (`StoragePredicateOperator::Eq`) in the generated `StorageListQuery` —
  there's no way to express `>`/`<`/`LIKE`/etc. today.
- A route's `:name` path parameter reaches a filtered field exactly the
  way it reaches any other input field — by name match against the
  guessed/declared input (see [conversions.md §6](conversions.md#6-http-path-parameter-conversion)).
  No extra wiring is needed: `GET "/spots/:spot_id/ratings"` on a use case
  filtering by `spot_id` "just works".
- Mutually exclusive with `ai`/`blob` (which don't have a `list`-shaped
  `call:` to scope in the first place).

See [conversions.md §5c](conversions.md#5c-filters-and-storagelistquery-predicates)
for the exact generated shape.

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

### `ai`
Switches use case implementation to AI-authored function body generation.
Mutually exclusive with `call` and `blob` — a use case declaring more than
one is a hard error (`cannot declare more than one of 'call', 'ai', 'blob'
— they're mutually exclusive`).

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
Route-level authorization requirement.

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
