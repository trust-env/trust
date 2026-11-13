# `logic {}` — use case bodies without Rust

`logic { ... }` writes a use case body in a small scripting language that
Trust **type-checks itself** and translates to Rust. It covers what most
[`code {}`](keywords.md#code) bodies do — read and write storages, call an
HTTP API, run another use case, compute, fail with a typed error — without
writing Rust, and without the risks of pasting Rust into the build.

```tr
use_case CloseTaskUseCase {
    input: CloseRequest
    output: Task
    transaction: true
    logic {
        let task = TaskInfra.get(input.id)          // NotFound when missing
        if task.status == TaskStatus.done {
            fail AlreadyDone("task {task.id} is already done")
        }
        return TaskInfra.update(Task { status: TaskStatus.done, ..task })
    }
}
```

Routes map its errors like any other use case's (`status { AlreadyDone -> 409 }`).
A complete project using most of the language is in
[`examples/logic_example.tr`](../examples/logic_example.tr).

## Why not Rust

| | `code {}` (Rust) | `logic {}` |
|---|---|---|
| Checked | when the generated crate is built, errors pointing into generated files | by `trust`, errors at `file.tr:line:col` |
| Needs to know | generated names (`{Name}Error`, `self.cart_infra`, `StorageListQuery`), `async`/`.await?`, ownership | the `.tr` types, infras and use cases |
| Can reach | anything: files, processes, env, network, any crate (whose `build.rs` runs at build time), `unsafe` | the infras and use cases of the project, and a fixed list of functions |
| Can loop forever / panic | yes | no `while`, no recursion; integer division and list access can't panic |
| Survives a generator change | maybe | yes: Trust writes the Rust |

`code {}` stays available for what `logic` can't express yet.

## Rules of the language

- `input` (the use case's input) and `user` (the caller's id, `0` without
  `auth`) are always in scope.
- `let x = ...` can't change; `var x = ...` can (`x = ...`, `x.field = ...`,
  `xs.push(...)`). A list can't change while a `for` walks it.
- Every path ends with `return <output>` or `fail <Error>` — checked.
- Types are those of the project: `Int Float Decimal Bool String Uuid
  DateTime Date Time Json`, `List<T>`, `Option<T>`, declared `type`s and
  `enum`s. Annotate with `let x: List<Int> = []` when Trust can't guess.
- No `null`: an `Option` must be unwrapped before use — `x ?? default`,
  `x ?? fail NotFound`, or `if let v = x { ... } else { ... }`. A plain `T`
  fits where an `Option<T>` is expected.
- `;` and newlines both separate statements; `//` starts a comment.

## Statements

| Statement | Meaning |
|---|---|
| `let x = e` / `var x: T = e` | Declare. |
| `x = e`, `x.f = e` | Assign a `var`. |
| `if c { } else if c { } else { }` | Conditions are `Bool`. |
| `if let v = opt { } else { }` | Runs with `v` when the Option is set. |
| `for x in list { }` | The only loop. |
| `return e` | Ends with the output (`T` must match `output:`). |
| `fail Name` / `fail Name("message {x}")` | Ends with an error. `NotFound` and `StorageError` always exist; any other name becomes a variant of the use case's error enum, mappable with [`status`](keywords.md#status). |
| `Infra.delete(id)`, `xs.push(x)` | A call run for its effect. |

## Expressions

| Expression | Meaning |
|---|---|
| `1`, `2.5`, `true`, `none`, `"text {expr}"` | Literals. Strings interpolate `{expr}` (strings may appear inside: `"at {t.format("HH:mm")}"`); `{{` `}}` are braces. |
| `[a, b]` | A `List`. With one item per line, commas may be left out. |
| `Task { title: t, done: false }` | A struct. Option, List and `id` fields may be left out, and so may the owner field of an entity whose infra has `owner:` (`create`/`update` fill it). `..base` copies the other fields: `Task { done: true, ..task }`. |
| `{ key: value, other }` | A `Json` object (`{ other }` is `{ other: other }`). |
| `Status.done` | An enum value (as written in the `enum`). |
| `Change.move { id: 1, start: t }`, `Change.reset` | A value of an enum whose values carry fields (Option and List fields may be left out). |
| `{}` | An empty `Map` (where one is expected: `var m: Map<Int, Float> = {}`). |
| `+ - * / %` | Numbers of one type (`.to_float()` converts). `+` also joins strings. Integer `/` by zero fails with `StorageError`. |
| `== != < <= > >=` | Comparisons (no chaining). |
| `and or not`, `x in list`, `x not in list` | Logic. |
| `a ?? b`, `a ?? fail E` | Default of an Option. When `b` is itself an `Option`, the result is an `Option` (first one set). `??` groups to the right: `a ?? b ?? fail E` is `a ?? (b ?? fail E)`. |

## Infras

Infras are used by name; the use case gets the ones its script uses.

**psql** (`entity: Task`):

| Call | Returns |
|---|---|
| `TaskInfra.find(id)` | `Option<Task>` |
| `TaskInfra.get(id)` | `Task`, or fails `NotFound` |
| `TaskInfra.query(cond, ..., order_by: f, order_by_desc: f, limit: n, offset: n)` | `List<Task>` (1000 rows by default) |
| `TaskInfra.all()` | `List<Task>` |
| `TaskInfra.create(task)` | the saved `Task` |
| `TaskInfra.update(task)` | the updated `Task` (by its `id`) |
| `TaskInfra.delete(id)` | the deleted `Task` |

Query conditions name a field of the entity on the left:
`status != Status.done`, `points >= input.min`, `id in ids`,
`title.contains("x")`, `title.like("a%")`, `title.ilike("a%")`.

When the infra declares `owner: user_id`, every call is **scoped to the
caller**: `find`/`get`/`update`/`delete` don't see other users' rows (they
read as `NotFound`), `query`/`all` only return the caller's, and
`create`/`update` stamp `user_id = user`. `owner: none` on the use case
turns that off (an admin task, a schedule).

`transaction: true` on the use case runs the whole script in one database
transaction, as with `code {}`.

**http_client**:

| Call | Returns |
|---|---|
| `Api.get(path)`, `Api.delete(path)` | the response |
| `Api.post(path, body)`, `put`, `patch` | the response |
| `Api.text(path)` | `String` |
| `Api.get_paged(path, "items", "nextPageToken")` | every page's `items`, following `nextPageToken` (`List<Json>`, or `List<T>` when the context expects it) |
| `Api.send("PATCH", path, body)`, `Api.send("DELETE", path)` | a `Response` whatever the HTTP status: `.status` (`Int`), `.text`, `.json()` |
| `Api.is_configured()` | `Bool`: its `${VARS}` are set |

The method of `send` is a literal: `"GET"`, `"POST"`, `"PUT"`, `"PATCH"`,
`"DELETE"`. Only a network failure makes it fail.

The response decodes into the type the context expects —
`let r: Purchase = Api.post(...)`, `return Api.get(...)` — and is `Json`
otherwise. `Json` values read with `j.field`, `j.get("key")`, then
`.str() .int() .float() .bool()` (`Option`s), `.list()`, `.is_null()`.

**ics_feed**: `Ics.occurrences(from, to)` returns the feed's
`List<IcsOccurrence>` over the window (fields `uid instance_id title start end
location description all_day`); `Ics.is_configured()` tells whether its URL
is set.

**Other use cases**: `PlanUseCase.run(input)` returns its output (its
`input` and `output` must be declared). That includes the use cases Trust
adds itself — a mirror's `{Name}MirrorUseCase`, a resource's
`Create{Entity}UseCase`, a journal's `Toggle{Journal}UseCase`. Other infra kinds (`s3`,
`grpc_client`) aren't available yet: use `code {}`.

## Recovering from errors (`try`)

`try e` turns `e`'s failure into a value instead of ending the script — for
reports where each source succeeds or fails on its own:

```tr
let google = try ImportGoogleUseCase.run(input)
if let error = google.error { log_warn("google: {error}") }
let report = google ?? SourceReport { name: "google", configured: true, written: 0, error: google.error }
```

A `Try<T>` has `.ok` (`Option<T>`), `.error` (`Option<String>`: the
message of `fail X("...")`, or `"not found"`), `is_ok()` and `is_error()`,
and `try e ?? fallback` is the value or the fallback. `try` covers any
expression (use case runs, infra and HTTP calls, a `fail` inside it); it
can't be used inside `x => ...`.

## Shared Rust functions (`extern fn`)

Rules several scripts — or Rust code, like a psql `defaults {}` — share
stay in one Rust file. A top-level `logic {}` block declares them with
their types:

```tr
logic {
    extern fn guess_domain(text: &String, at: DateTime) -> Domain from "./shared/rules.rs"
    extern fn guess_estimate(text: &String) -> Int from "./shared/rules.rs"
}
```

```rust
// shared/rules.rs
pub fn guess_domain(text: &str, at: chrono::DateTime<chrono::Utc>) -> crate::Domain { ... }
pub fn guess_estimate(text: &str) -> i64 { ... }
```

Every script of the project can then call `guess_domain(item.text, now())`;
Trust checks each call against the declaration and includes the file in
the crate of the use cases that use it (like `code { include }`: its `pub`
items are at the crate root, `crate::*` is in scope). `&T` passes the value
by reference (`&String` fits a Rust `&str`); the path is relative to the
`.tr` file (a path that only exists from the working directory is accepted too, like every path of a `.tr` file — the examples use that so they also work when read from the repository root). The declaration is trusted: Trust doesn't read the Rust, so a
wrong signature is reported by rustc, and the function can do anything Rust
can — keep these files small and pure. They are callable inside
`x => ...` (they can't fail).

## Methods and functions

| On | Methods |
|---|---|
| any printable value | `to_string()` |
| `String` | `len() is_empty() trim() lower() upper() contains(s) starts_with(s) ends_with(s) split(sep) replace(a, b) take(n) skip(n) to_int() to_float()` (`take`/`skip` count characters) |
| `Int` | `to_float() abs()` |
| `Float` | `round() floor() ceil()` (to `Int`), `abs()` |
| `DateTime` | `add_days(n) add_hours(n) add_minutes(n) add_seconds(n) minutes_since(t) seconds_since(t)`, and in the app's time zone: `date() weekday() hour() minute() day() month() year()` |
| `Date` | `add_days(n) days_since(d) weekday() day() month() year()` |
| a type with a [`map`](keywords.md#map) | `item.to_task()` for `map Item -> Task` |
| `Option` | `is_some() is_none()` |
| `List` | `len() is_empty() first() last() get(i) contains(x) take(n) skip(n) reverse() distinct() sum() min() max() join(sep) push(x)`, and with `x => ...`: `map filter filter_map any all find count sort_by sort_by_desc group_by to_map` |
| `Map<K, V>` | `len() is_empty() keys() values() get(k)` (`Option<V>`) `contains(k)`, and on a `var`: `set(k, v) remove(k)` |
| an enum with fields | `c.is(Change.move)` |
| `RawRequest` | `body`, `header("name")` |

Functions: `now()`, `today()` (the date in the app's `config { tz }`, UTC
without one), `journal("text {x}")` (the text of the undo-journal entry of a
use case declaring `journal:`), `env("UPPER_CASE_NAME")` (a literal
name, so every variable a script reads shows in the source; fails with
`StorageError` when unset), `sha256(text)`, `hmac_sha256(key, text)` (hex),
`secure_eq(a, b)` (constant-time comparison, for signatures),
`to_json(value)`, `min(a, b)`, `max(a, b)`, and:

| Function | Returns |
|---|---|
| `env_opt("NAME")` | `Option<String>`: none when unset or blank |
| `url_encode(text)` | `text` percent-encoded for a URL segment or query value |
| `parse_datetime(text)` | `Option<DateTime>` from RFC 3339, or a date alone (`2026-01-02`, midnight in the app's time zone) |
| `parse_date(text)` | `Option<Date>` from a date or an RFC 3339 instant |
| `week_start()` | this week's Monday 0:00 in the app's time zone |
| `log_warn(text)` | logs a warning (`tracing`), tagged with the use case |

`weekday()` is 1 for Monday to 7 for Sunday. `t.format("EEE dd/MM HH:mm")`
(on a `DateTime` or a `Date`) writes it in the app's time zone and language
— it needs `config { locale }`.

`filter_map(x => optional)` keeps the set values; `group_by(x => key)` gives a
`Map<K, List<T>>`, `to_map(x => key)` a `Map<K, T>` (the last item of a key
wins). A `Map`'s keys are `Int`, `String`, `Bool`, `Date`, `DateTime` or
`Uuid`, kept in order.

An enum whose values carry fields is read with `if let`, which also names
the fields it needs:

```tr
if let Change.move { event_id, start, end: until } = change {
    journal_entry("moved to {start.format("EEE HH:mm")}", requires_ack: true)
} else if let Change.reset = change {
    ...
}
```

Within a use case declaring `journal:`, `journal("text")` names the current
undo-journal entry and `journal_entry("text", requires_ack: true)` starts a
new one: the writes that follow belong to it.

**Notifications**: the router a `notifications Notify { ... }` block declares
sends with `Notify.send(kind, text)`, and `Notify.send_recorded(kind, text)`
also records the notification in its `infra:` and returns the row (`kind`
is a `String` or an enum value). Dates and instants compare
with `< <= > >= ==`; `d.add_days(-1)` goes back.

Inside `x => ...` nothing that can fail runs (infra and use case calls,
`fail`, `try`, integer division, `env`, `hmac_sha256`): write a `for` loop instead.

```tr
use_case WebhookUseCase {
    input: RawRequest
    output: WebhookAck
    logic {
        let signature = input.header("x-signature") ?? fail InvalidSignature("missing signature")
        let expected = hmac_sha256(env("WEBHOOK_SECRET"), input.body)
        if not secure_eq(signature, expected) { fail InvalidSignature }
        return WebhookAck { received: input.body.len() }
    }
}
```

## What it becomes

The script is lowered into the `code {}` use case it stands for: same
error enum, struct, constructor, transaction handling, route mapping and
`main.rs` wiring. The generated body (`applications/<use_case>.rs`) is
plain Rust that compiles without warnings; values are cloned when read, so
a script never deals with borrowing.
