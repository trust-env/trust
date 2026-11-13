# Recipes

## Recipe 1: Basic CRUD HTTP Service

```tr
type Task {
    id: Int,
    title: String
}

infra TaskInfra {
    storage {
        psql {
            entity: Task
        }
    }
}

use_case CreateTaskUseCase {
    call: TaskInfra.save_one
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            POST "/tasks" {
                use_case: CreateTaskUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}
```

## Recipe 2: Add gRPC Alongside HTTP

```tr
adapter InternalGrpcApi {
    grpc {
        services {
            rpc CreateTask {
                use_case: CreateTaskUseCase
            }
        }
        on: "0.0.0.0:50051"
    }
}

app Main: run {
    HttpApi
    InternalGrpcApi
}
```

## Recipe 3: Cross-Binary Call via grpc_client

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

use_case GetRemoteTaskUseCase {
    call: RemoteTaskInfra.find_one
}
```

## Recipe 4: Route-Level Auth + Status Mapping

```tr
DELETE "/tasks/:id" {
    use_case: DeleteTaskUseCase
    auth {
        infra: PermissionInfra
        target: "task"
        action: "delete"
    }
    status {
        InternalError -> 500
        BadRequest -> 400
    }
}
```

## Recipe 5: AI-Authored Use Case

```tr
use_case ValidateEmailUseCase {
    input: ValidateEmailRequest
    output: ValidateEmailResponse
    ai {
        prompt: 'Validate email and return response.'
    }
}
```

Compile with AI settings:

```bash
cargo run -- app.tr --ai-provider=anthropic --ai-api-key=YOUR_KEY
```

## Recipe 6: Plugin Keyword Usage

```bash
cargo run -- app.tr --plugin=/abs/path/libnote_plugin.so
```

Then use the plugin keyword in `.tr`:

```tr
note {
    message: "hello from plugin"
}
```

## Recipe 7: Multi-App Workspace with Cross-Binary gRPC

Two independent binaries in one `.tr` file, wired together over gRPC. This
generates a Cargo workspace with two member crates under `out/`.

```tr
type Task {
    id: Int,
    title: String
}

infra TaskInfra {
    storage {
        psql {
            entity: Task
        }
    }
}

use_case GetTaskUseCase {
    call: TaskInfra.find_one
}

adapter WorkerGrpcApi {
    grpc {
        services {
            rpc GetTask {
                use_case: GetTaskUseCase
            }
        }
        on: "0.0.0.0:15051"
    }
}

app Worker: run {
    WorkerGrpcApi
}

infra RemoteTaskInfra {
    storage {
        grpc_client {
            service: WorkerGrpcApi
            on: "127.0.0.1:15051"
            entity: Task
            methods {
                find_one: GetTask
            }
        }
    }
}

use_case GetRemoteTaskUseCase {
    call: RemoteTaskInfra.find_one
}

adapter PublicHttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            GET "/tasks/:id" {
                use_case: GetRemoteTaskUseCase
            }
        }
    }
}

app Gateway: run {
    PublicHttpApi
}
```

`rpc GetTask` deliberately omits `input:`/`output:` — those fields would
only be checked against `GetTaskUseCase`'s own signature, never define it
(see [conversions.md §10](conversions.md#10-routerpc-inputoutput-are-a-cross-check-not-the-source-of-truth)),
so leaving them off here is idiomatic, not incomplete.

`Worker` and `Gateway` compile to two independent binaries (`out/worker/`,
`out/gateway/`) under one workspace root `Cargo.toml`. `Gateway` never
constructs `Worker`'s adapter directly — it calls it over a real gRPC
connection at compile-known address `127.0.0.1:15051`, so both binaries must
actually be running for the end-to-end path to work. See
`scenarios/05-cross-binary-grpc/` for a fully live version of this pattern
(real Postgres, both binaries spawned as processes, a real HTTP round-trip).

## Recipe 8: MinIO/S3 Blob Storage (Presigned Upload/Download URLs)

```tr
type MediaObject {
    key: String,
    content_type: String
}

infra MediaInfra {
    storage {
        s3 {
            bucket: "media"
            entity: MediaObject
        }
    }
}

use_case RequestUploadUrlUseCase {
    blob: MediaInfra.presigned_put_url
}

use_case RequestDownloadUrlUseCase {
    blob: MediaInfra.presigned_get_url
}

use_case DeleteMediaUseCase {
    blob: MediaInfra.delete
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            POST "/media/upload-url" {
                use_case: RequestUploadUrlUseCase
            }
            GET "/media/download-url" {
                use_case: RequestDownloadUrlUseCase
            }
            DELETE "/media" {
                use_case: DeleteMediaUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}
```

The mobile/web client calls `POST /media/upload-url` with `{ "key": "..." }`,
gets back `{ "url": "..." }`, and uploads the file bytes directly to that
presigned URL — the generated backend never sees or proxies the file
content. Same idea in reverse for downloads via `GET /media/download-url`.

Endpoint/access key/secret key are never in `.tr` source — set
`MEDIA_INFRA_S3_ENDPOINT`, `MEDIA_INFRA_S3_ACCESS_KEY`, and
`MEDIA_INFRA_S3_SECRET_KEY` in the environment (or run with `--dev`, which
scaffolds them into `.env` with local-MinIO defaults —
`http://localhost:9000` / `minioadmin` / `minioadmin`).

## Recipe 9: Dev Mode (Live Docs + Scaffolded `.env`)

```bash
cargo run -- app.tr --dev
```

For every HTTP adapter, this adds a `GET /app` route serving an HTML docs
page (routes, input/output fields, error/status table), and writes a
starter `.env` into that app's crate root with sniffed defaults for any
`${VAR}` found in `on:` fields, DB URLs for each `psql` infra, and
`JWT_SECRET=changeme` if any route uses `auth {}`. See
[cli.md](cli.md#dev-mode---dev) for the exact defaulting rules — treat the
scaffolded values as local-dev placeholders, not production secrets.

## Recipe 10: Filtered List ("X's children" Routes)

```tr
type Spot {
    id: Int,
    lat: Float,
    lng: Float
}

type Rating {
    id: Int,
    spot_id: Int,
    temperature: Int,
    safety: Int
}

infra RatingInfra {
    storage {
        psql {
            entity: Rating
        }
    }
}

use_case ListRatingsForSpotUseCase {
    call: RatingInfra.list
    filters {
        spot_id: input.spot_id
    }
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            GET "/spots/:spot_id/ratings" {
                use_case: ListRatingsForSpotUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}
```

No explicit `input` needed — the route's `:spot_id` path segment reaches
the use case because `filters {}` (with no declared `input`) auto-adds a
`spot_id: Int` field to the guessed `{ limit, offset }` list input, and the
existing path-param-by-name matching (see
[conversions.md §6](conversions.md#6-http-path-parameter-conversion)) picks
it up automatically. `limit`/`offset` arrive via the **query string**
(`GET /spots/:spot_id/ratings?limit=20&offset=0`), not a JSON body — `GET`
routes always read their non-path fields that way, since a real HTTP
client refuses to send a body on `GET` at all.

---

The recipes below cover the features added for real apps (an assistant
that plans the week, syncs calendars and talks on Telegram). Each one is a
complete project: code blocks start with a `// path` comment naming the file
they go in. Compile the entry file with `trust app.tr --dev`, then
`cd out && cargo run`; protected routes take a JWT printed by
`cargo run -- token 1` (user 1).

## Recipe 11: A Project Split Into Modules

```tr
// app.tr
config { json_case: "camel" }

include "./modules/tasks.tr"
include "./adapters/api.tr"

app Main: run { Api }
```

```tr
// modules/tasks.tr
type Task {
    id: Int
    user_id: Int
    title: String
    done: Bool
}

infra TaskInfra { storage { psql { entity: Task owner: user_id } } }

resource Tasks { infra: TaskInfra path: "/tasks" list create get patch delete }
```

```tr
// adapters/api.tr
adapter Api {
    http {
        on: "0.0.0.0:${PORT:-8080}"
        auth: required
        resources { Tasks }
    }
}
```

- `include` paths are relative to the file that writes them, so a module
  can bring its own Rust files (`code { module: "./x.rs" }`) and seeds.
- `resource` generates five use cases (`ListTasksUseCase`,
  `CreateTaskUseCase`, ...) and their routes; `owner: user_id` on the infra
  makes every one of them per-user (another user's task is a 404).
- `auth: required` on the adapter protects every route; a route opts out
  with `auth: none`.

```bash
curl -X POST localhost:8080/tasks -H "Authorization: Bearer $TOKEN" -d '{"title":"Call the bank","done":false}'
curl -X PATCH localhost:8080/tasks/1 -H "Authorization: Bearer $TOKEN" -d '{"done":true}'
curl localhost:8080/tasks -H "Authorization: Bearer $TOKEN"
```

## Recipe 12: Smart Creation — Defaults, Bounds and Derived Timestamps

One `PUT /tasks` that creates (without `id`) or patches (with `id`), so a
form — or an AI agent — sends only what it knows:

```tr
// app.tr
enum Status { todo doing done }

type Task {
    id: Int
    user_id: Int
    title: String
    status: Status
    priority: Int(1..4)
    domain: String
    created_at: DateTime
    done_at: Option<DateTime>
}
type CountRequest { status: Option<Status> }
type Count { total: Int }

infra TaskInfra {
    storage {
        psql {
            entity: Task
            owner: user_id
            defaults { status: "todo"  priority: 3  domain: guess_domain(title) }
            stamps { done_at: status == "done" }
        }
    }
}

resource Tasks { infra: TaskInfra path: "/tasks" list save delete }

// Any code use case can share Rust with the defaults through `include`.
use_case CountTasksUseCase {
    input: CountRequest
    output: Count
    code { module: "./count.rs" include: "./rules.rs" infra: TaskInfra }
}

adapter Api {
    http {
        on: "0.0.0.0:8080"
        auth: required
        resources { Tasks }
        routes { GET "/tasks/count" { use_case: CountTasksUseCase } }
    }
}
app Main: run { Api }
```

```rust
// rules.rs
/// Guesses a task's domain from its title.
pub fn guess_domain(title: &str) -> String {
    let work = ["meeting", "client", "report"];
    if work.iter().any(|w| title.to_lowercase().contains(w)) { "work" } else { "personal" }.to_string()
}
```

```rust
// count.rs
use crate::*;

pub async fn execute(deps: &impl CountTasksUseCaseDeps, user: i64, input: CountRequest) -> Result<Count, CountTasksUseCaseError> {
    let mut query = Query::new().eq("user_id", user);
    if let Some(status) = input.status {
        query = query.eq("status", status);
    }
    Ok(Count { total: deps.task_infra().query(query).await?.len() as i64 })
}
```

```bash
# Create: status, priority, domain and created_at are filled in.
curl -X PUT localhost:8080/tasks -H "Authorization: Bearer $TOKEN" -d '{"title":"Client meeting notes"}'
# Patch: only what is sent changes; done_at is set by the database.
curl -X PUT localhost:8080/tasks -H "Authorization: Bearer $TOKEN" -d '{"id":1,"status":"done"}'
# 422: priority must be between 1 and 4.
curl -X PUT localhost:8080/tasks -H "Authorization: Bearer $TOKEN" -d '{"title":"x","priority":9}'
```

- A creation without a required field that has no default is a 422
  (`MissingField`).
- Sending `"status":"todo"` again clears `done_at`.
- `null` clears an optional field when patching.

## Recipe 13: One Settings Row per User

```tr
// app.tr
type Settings {
    id: Int
    user_id: Int
    day_start: Time
    day_end: Time
    margin_min: Int(0..120)
}

infra SettingsInfra { storage { psql { entity: Settings owner: user_id } } }

resource Preferences {
    infra: SettingsInfra
    path: "/settings"
    owner: user_id
    singleton
    default { day_start: "08:00:00"  day_end: "20:00:00"  margin_min: 10 }
    get
    upsert
}

adapter Api { http { on: "0.0.0.0:8080" auth: required resources { Preferences } } }
app Main: run { Api }
```

- `GET /settings` answers the caller's row, or the defaults before the
  first save; `PUT /settings` creates or replaces it (one row per user,
  enforced by a unique index).
- From a `code {}` use case: `deps.settings_infra().get_or_default("user_id", user).await?`.

## Recipe 14: Undoable Actions

Every write of a journaled use case is recorded with the rows before and
after; one route undoes or redoes an entry.

```tr
// app.tr
type Task { id: Int  user_id: Int  title: String  done: Bool }
type Action {
    id: Int
    user_id: Int
    kind: String
    text: String
    changes: Json
    undone: Bool
    at: DateTime
    requires_ack: Bool
}
type CompleteAllRequest { prefix: String }
type CompleteAllResult { completed: Int  action_id: Option<Int> }

infra TaskInfra { storage { psql { entity: Task owner: user_id } } }
infra ActionInfra { storage { psql { entity: Action owner: user_id } } }

journal Actions { infra: ActionInfra owner: user_id undo: "/actions/:id/toggle" conflict: refuse }

resource Tasks { infra: TaskInfra path: "/tasks" journal: Actions list create patch delete }
resource ActionLog { infra: ActionInfra path: "/actions" list { order_by: id desc } }

use_case CompleteAllUseCase {
    input: CompleteAllRequest
    output: CompleteAllResult
    journal: Actions
    text: "Completed every task starting with {input.prefix}"
    code { module: "./complete_all.rs" infra: TaskInfra }
}

adapter Api {
    http {
        on: "0.0.0.0:8080"
        auth: required
        resources { Tasks ActionLog Actions }
        routes { POST "/tasks/complete" { use_case: CompleteAllUseCase } }
    }
}
app Main: run { Api }
```

```rust
// complete_all.rs
use crate::*;

pub async fn execute(deps: &impl CompleteAllUseCaseDeps, user: i64, input: CompleteAllRequest) -> Result<CompleteAllResult, CompleteAllUseCaseError> {
    let tasks = deps.task_infra().query(Query::new().eq("user_id", user).eq("done", false)).await?;
    let mut completed = 0;
    for task in tasks.into_iter().filter(|t| t.title.starts_with(&input.prefix)) {
        deps.task_infra().patch_one(task.id, Some(("user_id", user)), TaskPatch { done: Some(true), ..Default::default() }).await?;
        completed += 1;
    }
    // action_id is filled in by Trust with the journal entry's id.
    Ok(CompleteAllResult { completed, action_id: None })
}
```

```bash
curl -X POST localhost:8080/tasks/complete -H "Authorization: Bearer $TOKEN" -d '{"prefix":"Buy"}'
# → {"completed":2,"actionId":3}
curl -X POST localhost:8080/actions/3/toggle -H "Authorization: Bearer $TOKEN"   # undo
curl -X POST localhost:8080/actions/3/toggle -H "Authorization: Bearer $TOKEN"   # redo
curl "localhost:8080/actions" -H "Authorization: Bearer $TOKEN"                  # history
```

- The resource's create, patch and delete are journaled too: each request
  is one entry.
- A row changed since the entry makes the undo a 409 (`conflict: refuse`);
  a row already back in the target state (deleted by something else, say)
  is simply left alone.
- In code, `deps.journal().entry("Moved « x »").requires_ack(true)` starts a
  separate entry for the writes that follow (one undoable entry per move).

## Recipe 15: Hand-Written Logic as a Tested Module

```tr
// app.tr
config { tz: "Europe/Paris" }

type Task { id: Int  user_id: Int  title: String  priority: Int(1..4)  due: Option<DateTime> }
type NextRequest { limit: Option<Int> }
type Suggestion { task_id: Int  title: String  score: Int }
type Suggestions { items: List<Suggestion> }

infra TaskInfra { storage { psql { entity: Task owner: user_id } } }

use_case SuggestNextUseCase {
    input: NextRequest
    output: Suggestions
    code {
        module: "./suggest_next.rs"
        include: "./score.rs"
        infra: TaskInfra
    }
}

adapter Api { http { on: "0.0.0.0:8080" auth: required routes { GET "/next" { use_case: SuggestNextUseCase } } } }
app Main: run { Api }
```

```rust
// score.rs
/// Higher is more urgent: priority 1 counts most, then the due date.
pub fn score(priority: i64, due_in_days: Option<i64>) -> i64 {
    let urgency = match due_in_days {
        Some(days) if days <= 0 => 50,
        Some(days) if days <= 2 => 30,
        Some(_) => 10,
        None => 0,
    };
    (5 - priority) * 10 + urgency
}

#[cfg(test)]
mod tests {
    #[test]
    fn overdue_beats_priority() {
        assert!(super::score(3, Some(-1)) > super::score(1, None));
    }
}
```

```rust
// suggest_next.rs
use crate::*;

pub async fn execute(deps: &impl SuggestNextUseCaseDeps, user: i64, input: NextRequest) -> Result<Suggestions, SuggestNextUseCaseError> {
    let today = crate::tz::today();
    let tasks = deps.task_infra().query(Query::new().eq("user_id", user)).await?;
    let mut items: Vec<Suggestion> = tasks
        .into_iter()
        .map(|t| {
            let due_in = t.due.map(|d| (crate::tz::local_date(d) - today).num_days());
            Suggestion { task_id: t.id, title: t.title, score: score(t.priority, due_in) }
        })
        .collect();
    items.sort_by_key(|s| -s.score);
    items.truncate(input.limit.unwrap_or(3) as usize);
    Ok(Suggestions { items })
}
```

- `module:` files are real Rust modules: helpers, `#[cfg(test)]` tests (run
  them with `cargo test` in `out/`), and editor support once generated.
- `include:` files are shared by every `code {}` use case of the app.
- `infra: [A, B]`, `use_case: [Other, More]` (to call other use cases) and
  `deps { crate: "1" }` complete the list; storage errors convert with `?`.

## Recipe 16: Mirror a Published Outlook Calendar

```tr
// app.tr
config { tz: "Europe/Paris" }

enum EventKind { outlook manual }
type CalendarEvent {
    id: Int
    user_id: Int
    kind: EventKind
    external_id: String
    title: String
    start_at: DateTime
    end_at: DateTime
    note: Option<String>
}
type CalendarRequest { after: DateTime  before: DateTime }

infra EventInfra { storage { psql { entity: CalendarEvent owner: user_id } } }
infra OutlookIcs { ics_feed { url: "${OUTLOOK_ICS_URL}" timezone_fallback: "Europe/Paris" retry: 3 } }

mirror OutlookMirror {
    source: OutlookIcs
    into: EventInfra
    key: [user_id, kind, external_id]
    scope { kind: "outlook"  end_at > window.from  start_at < window.to }
    map {
        kind: "outlook"
        external_id: item.instance_id
        title: item.title
        start_at: item.start
        end_at: item.end
        note: item.location
    }
    window { from: week_start  days: 28 }
}

use_case ListCalendarUseCase {
    input: CalendarRequest
    call: EventInfra.list
    filters { end_at > input.after  start_at < input.before }
    order_by: start_at
}

schedule Sync { user: 1  "*/15 * * * *" => OutlookMirrorUseCase }

adapter Api {
    http {
        on: "0.0.0.0:8080"
        auth: required
        routes {
            POST "/calendar/sync" { use_case: OutlookMirrorUseCase }
            GET "/calendar" { use_case: ListCalendarUseCase }
        }
    }
}
app Main: run { Api Sync }
```

- `OUTLOOK_ICS_URL` is the calendar's published ICS link (Outlook:
  Settings › Calendar › Shared calendars › Publish a calendar).
- Every 15 minutes (and on `POST /calendar/sync`), the four weeks from
  Monday are read: recurring meetings expanded, moved and cancelled
  occurrences handled, each occurrence upserted on
  `(user_id, kind, external_id)`, and the `outlook` rows of that window no
  longer in the feed deleted. `manual` events are never touched.
- The answer is `{ "configured", "written", "removed", "error" }`; an empty
  URL is `configured: false`, a feed error is reported and deletes nothing.
- `POST /calendar/sync {"days": 7}` narrows the window.

## Recipe 17: Read a Google API With OAuth2 and Paging

A `code` use case pages through Google Calendar, and a mirror copies its
items — the same mirror as Recipe 16, with a use case as source.

```tr
// app.tr
config { tz: "Europe/Paris" }

enum EventKind { google }
type CalendarEvent {
    id: Int
    user_id: Int
    kind: EventKind
    external_id: String
    title: String
    start_at: DateTime
    end_at: DateTime
}
type GoogleWindow { from: DateTime  to: DateTime }
type GoogleItem { id: String  title: String  start: DateTime  end: DateTime }
type GoogleEvents { items: List<GoogleItem> }

infra EventInfra { storage { psql { entity: CalendarEvent owner: user_id } } }
infra GoogleCalendar {
    http_client {
        base_url: "https://www.googleapis.com/calendar/v3"
        oauth2 {
            token_url: "https://oauth2.googleapis.com/token"
            client_id: "${GOOGLE_CLIENT_ID}"
            client_secret: "${GOOGLE_CLIENT_SECRET}"
            refresh_token: "${GOOGLE_REFRESH_TOKEN}"
        }
        retry: 3
    }
}

use_case GoogleEventsUseCase {
    input: GoogleWindow
    output: GoogleEvents
    code { module: "./google_events.rs" infra: GoogleCalendar }
}

mirror GoogleMirror {
    source: GoogleEventsUseCase
    into: EventInfra
    key: [user_id, kind, external_id]
    scope { kind: "google"  end_at > window.from  start_at < window.to }
    map { kind: "google"  external_id: item.id  title: item.title  start_at: item.start  end_at: item.end }
    window { from: today  days: 14 }
}

adapter Api { http { on: "0.0.0.0:8080" auth: required routes { POST "/calendar/google/sync" { use_case: GoogleMirrorUseCase } } } }
app Main: run { Api }
```

```rust
// google_events.rs
use crate::*;

pub async fn execute(deps: &impl GoogleEventsUseCaseDeps, _user: i64, input: GoogleWindow) -> Result<GoogleEvents, GoogleEventsUseCaseError> {
    let api = deps.google_calendar();
    if !api.is_configured() {
        return Ok(GoogleEvents { items: Vec::new() });
    }
    let path = format!(
        "/calendars/primary/events?singleEvents=true&timeMin={}&timeMax={}",
        input.from.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        input.to.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    );
    // Every page, following nextPageToken; the bearer token is added and renewed by Trust.
    let raw: Vec<serde_json::Value> = api.get_json_paged(&path, "items", "nextPageToken").await?;
    let items = raw
        .iter()
        .filter_map(|e| {
            let at = |key: &str| e[key]["dateTime"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
            Some(GoogleItem {
                id: e["id"].as_str()?.to_string(),
                title: e["summary"].as_str().unwrap_or("(untitled)").to_string(),
                start: at("start")?.with_timezone(&chrono::Utc),
                end: at("end")?.with_timezone(&chrono::Utc),
            })
        })
        .collect();
    Ok(GoogleEvents { items })
}
```

- The refresh token comes from Google's OAuth consent flow (scope
  `calendar.readonly`); Trust exchanges it for access tokens, caches them,
  and renews them before they expire or after a 401.
- The source use case receives the mirror's window as `{ from, to }` and
  returns `items`; all-day events (no `dateTime`) are skipped here.

## Recipe 18: Notifications With Routing, Fallback and History

```tr
// app.tr
enum NotificationKind { brief alert }
enum NotificationStatus { sent failed logged }
type Notification {
    id: Int
    user_id: Int
    kind: NotificationKind
    at: DateTime
    channel: String
    text: String
    status: NotificationStatus
    error: Option<String>
    external_id: Option<String>
}
type SendRequest { kind: NotificationKind  text: String }

infra NotificationInfra { storage { psql { entity: Notification owner: user_id } } }

channel Telegram { telegram { token: "${TELEGRAM_BOT_TOKEN}" chat: "${TELEGRAM_CHAT_ID}" } }
channel Team { webhook { url: "${DISCORD_WEBHOOK_URL}" format: discord } }

notifications Notify {
    infra: NotificationInfra
    default: Telegram
    fallback: log
    routing { alert: [Team, Telegram] }
}

use_case SendUseCase {
    input: SendRequest
    output: Notification
    code { module: "./send.rs" infra: [Notify, NotificationInfra] }
}
use_case ListNotificationsUseCase { call: NotificationInfra.list order_by: at desc }

adapter Api {
    http {
        on: "0.0.0.0:8080"
        auth: required
        routes {
            POST "/notify" { use_case: SendUseCase }
            GET "/notifications" { use_case: ListNotificationsUseCase }
        }
    }
}
app Main: run { Api }
```

```rust
// send.rs
use crate::*;

pub async fn execute(deps: &impl SendUseCaseDeps, user: i64, input: SendRequest) -> Result<Notification, SendUseCaseError> {
    deps.notify()
        .send_recorded(deps.notification_infra(), user, input.kind.as_str(), &input.text)
        .await
        .map_err(SendUseCaseError::StorageError)
}
```

- A `brief` goes to Telegram; an `alert` to Discord first, Telegram if
  Discord fails. When every channel fails (or isn't configured), the text
  goes to the logs: the row says `"channel": "log", "status": "logged"`
  and keeps the errors.
- `deps.notify().send(kind, &text)` sends without recording;
  `send_to("Telegram", kind, &text, None)` targets one channel.

## Recipe 19: A Telegram Bot

Messages to the bot become notes; `/todo <text>` creates a task; a reply
to the evening review goes to the journal.

```tr
// app.tr
enum NotificationKind { brief review }
enum NotificationStatus { sent failed logged }
type Notification {
    id: Int  user_id: Int  kind: NotificationKind  at: DateTime  channel: String
    text: String  status: NotificationStatus  error: Option<String>  external_id: Option<String>
}
type Note { id: Int  user_id: Int  text: String  kind: String }
type TextInput { text: String }
type Saved { summary: String }

infra NotificationInfra { storage { psql { entity: Notification owner: user_id } } }
infra NoteInfra { storage { psql { entity: Note owner: user_id } } }

channel Telegram { telegram { token: "${TELEGRAM_BOT_TOKEN}" chat: "${TELEGRAM_CHAT_ID}" } }
notifications Notify { infra: NotificationInfra default: Telegram fallback: log }

use_case CaptureUseCase { input: TextInput output: Saved code { module: "./capture.rs" include: "./notes.rs" infra: NoteInfra } }
use_case TodoUseCase { input: TextInput output: Saved code { module: "./todo.rs" infra: NoteInfra } }
use_case JournalUseCase { input: TextInput output: Saved code { module: "./journal.rs" infra: NoteInfra } }

bot TelegramBot {
    channel: Telegram
    path: "/webhooks/telegram"
    secret: "${TELEGRAM_WEBHOOK_SECRET}"
    user: 1
    commands { todo: TodoUseCase  journal: JournalUseCase }
    on_reply { review: JournalUseCase }
    default: CaptureUseCase
    reply: "✓ {result.summary}"
}

adapter Api { http { on: "0.0.0.0:8080" auth: required resources { TelegramBot } } }
app Main: run { Api }
```

```rust
// notes.rs
// (`include:` files get `use crate::*` automatically.)

/// The note a bot message becomes.
pub fn note(user: i64, kind: &str, text: &str) -> Note {
    Note { id: 0, user_id: user, text: text.trim().to_string(), kind: kind.to_string() }
}

/// The acknowledgment sent back (`reply: "✓ {result.summary}"`).
pub fn saved(kind: &str, text: &str) -> Saved {
    Saved { summary: format!("{}: {}", kind, text.trim()) }
}
```

```rust
// capture.rs
use crate::*;

pub async fn execute(deps: &impl CaptureUseCaseDeps, user: i64, input: TextInput) -> Result<Saved, CaptureUseCaseError> {
    deps.note_infra().save_one(note(user, "note", &input.text)).await?;
    Ok(saved("note", &input.text))
}
```

```rust
// todo.rs
use crate::*;

pub async fn execute(deps: &impl TodoUseCaseDeps, user: i64, input: TextInput) -> Result<Saved, TodoUseCaseError> {
    if input.text.trim().is_empty() {
        return Ok(Saved { summary: "write the task after /todo".to_string() });
    }
    deps.note_infra().save_one(note(user, "todo", &input.text)).await?;
    Ok(saved("todo", &input.text))
}
```

```rust
// journal.rs
use crate::*;

pub async fn execute(deps: &impl JournalUseCaseDeps, user: i64, input: TextInput) -> Result<Saved, JournalUseCaseError> {
    deps.note_infra().save_one(note(user, "journal", &input.text)).await?;
    Ok(saved("journal", &input.text))
}
```

Register the webhook once with Telegram:

```bash
curl "https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/setWebhook" \
  -d url=https://example.com/webhooks/telegram -d secret_token=$TELEGRAM_WEBHOOK_SECRET
```

- Telegram's `X-Telegram-Bot-Api-Secret-Token` header must match (403
  otherwise); messages from any chat other than `TELEGRAM_CHAT_ID` are
  ignored; other commands (`/start`) too.
- The route always answers 200 (`{ handled, reply, error }`) so Telegram
  doesn't resend; a failing use case is reported in `error`.
- `on_reply` matches a reply to a notification recorded by `Notify` (here a
  `review` sent with `send_recorded`).

## Recipe 20: Give an AI Agent Your Use Cases (MCP)

```tr
// app.tr
type Task { id: Int  user_id: Int  title: String  done: Bool }

infra TaskInfra { storage { psql { entity: Task owner: user_id } } }

resource Tasks { infra: TaskInfra path: "/tasks" list create patch }

adapter Api { http { on: "0.0.0.0:8080" auth: required resources { Tasks } } }

adapter Agent {
    mcp {
        on: "127.0.0.1:${MCP_PORT:-8090}"
        auth: required
        instructions: "Task list of the user: read it before adding, never duplicate."
        tools {
            list_tasks { use_case: ListTasksUseCase description: "The user's tasks" }
            add_task { use_case: CreateTaskUseCase description: "Add a task (title, done)" }
            update_task { use_case: PatchTaskUseCase description: "Change a task's title or mark it done" }
        }
        resources {
            tasks { use_case: ListTasksUseCase uri: "app://tasks" description: "Every task" }
        }
    }
}

app Main: run { Api Agent }
```

Connect Claude Code over HTTP with a long-lived token:

```bash
claude mcp add --transport http tasks http://127.0.0.1:8090/mcp \
  --header "Authorization: Bearer $(./target/debug/main token 1)"
```

or let the client start the server itself (stdio):

```bash
claude mcp add tasks -e DATABASE_URL=$DATABASE_URL -e MCP_USER_ID=1 -- /path/to/out/target/debug/main mcp
```

- Each tool's input schema comes from the use case's input (bounds,
  enums, optional fields included); an error comes back as readable text.
- For claude.ai custom connectors (remote, OAuth), add
  `oauth { public_url: "${MCP_PUBLIC_URL}" password: "${MCP_OAUTH_PASSWORD}" user: 1 }`
  and serve it behind HTTPS: signing in asks for that password.
- Pair it with Recipe 14: an agent's writes become undoable actions.

## Recipe 21: A Morning Brief in French, at Local Time

```tr
// app.tr
config { tz: "Europe/Paris"  locale: "fr" }

type Task { id: Int  user_id: Int  title: String  due: Option<DateTime>  done: Bool }
type BriefRequest { send: Option<Bool> }
type Brief { text: String  sent: Bool }

infra TaskInfra { storage { psql { entity: Task owner: user_id } } }

channel Telegram { telegram { token: "${TELEGRAM_BOT_TOKEN}" chat: "${TELEGRAM_CHAT_ID}" } }
notifications Notify { default: Telegram fallback: log }

use_case MorningBriefUseCase {
    input: BriefRequest
    output: Brief
    code { module: "./brief.rs" infra: [TaskInfra, Notify] }
}

schedule Mornings {
    user: 1
    "30 7 * * mon-fri" => MorningBriefUseCase { send: true }
}

adapter Api { http { on: "0.0.0.0:8080" auth: required routes { GET "/brief" { use_case: MorningBriefUseCase } } } }
app Main: run { Api Mornings }
```

```rust
// brief.rs
use crate::*;
use crate::fmt::{fmt_date, fmt_relative_day, fmt_time, plural};

pub async fn execute(deps: &impl MorningBriefUseCaseDeps, user: i64, input: BriefRequest) -> Result<Brief, MorningBriefUseCaseError> {
    let today = crate::tz::today();
    // Everything due before the end of the week (local Monday-to-Sunday).
    let (_, week_end) = crate::tz::week_bounds(chrono::Utc::now());
    let tasks = deps
        .task_infra()
        .query(Query::new().eq("user_id", user).eq("done", false).lt("due", week_end.to_rfc3339()).order_by("due"))
        .await?;
    let mut text = format!("Bonjour ! Nous sommes {}.\n", fmt_date(today, "EEEE d MMMM"));
    text.push_str(&format!("{} cette semaine :\n", plural(tasks.len(), "tâche", "tâches")));
    for task in &tasks {
        let due = task.due.map(|d| format!("{} {}", fmt_relative_day(d), fmt_time(d))).unwrap_or_default();
        text.push_str(&format!("• {} — {}\n", task.title, due));
    }
    let sent = if input.send.unwrap_or(false) {
        deps.notify().send("brief", &text).await.status == "sent"
    } else {
        false
    };
    Ok(Brief { text, sent })
}
```

```text
Bonjour ! Nous sommes mardi 10 novembre.
2 tâches cette semaine :
• Rapport mensuel — en retard 18h
• Appeler la banque — demain 9h30
```

- The schedule runs at 7:30 Paris time all year (daylight saving
  included); `GET /brief` previews the text without sending.
- `crate::tz` gives `today()`, `now_local()`, `day_bounds(date)`,
  `week_bounds(t)`, `local_to_utc(naive)`; `crate::fmt` gives
  `fmt_date`, `fmt_time`, `fmt_relative_day` (and `fmt_relative_day_from`
  for a fixed "now" in tests) and `plural`.

## Recipe 22: Live Updates and a Typed Front-End Client

```tr
// app.tr
config { json_case: "camel" }

type Task { id: Int  user_id: Int  title: String  done: Bool }
infra TaskInfra { storage { psql { entity: Task owner: user_id } } }
resource Tasks { infra: TaskInfra path: "/tasks" list create patch }

adapter Api {
    http {
        on: "0.0.0.0:8080"
        cors: "${CORS_ORIGINS:-http://localhost:5173}"
        auth: required
        events: "/events"
        resources { Tasks }
    }
}
app Main: run { Api }
```

```bash
trust app.tr --emit-ts=../web/src/api.ts
```

```ts
// web/src/tasks.ts
import { ApiClient } from "./api";

const token = () => localStorage.getItem("token");
const api = new ApiClient({ baseUrl: "http://localhost:8080", token });

export async function addAndList(title: string) {
  await api.createTask({ title, done: false });     // typed input
  return (await api.listTasks({})).tasks;           // typed output: Task[]
}

// Refresh whenever any of this user's data changes (another tab, an agent, a schedule).
export function onChange(refresh: () => void) {
  const events = new EventSource(`http://localhost:8080/events?access_token=${token()}`);
  events.addEventListener("change", refresh);       // data: { use_case, user, at }
  return () => events.close();
}
```

- `--emit-ts` writes one interface per type and one client class per HTTP
  adapter (`ApiClient`), with path parameters, query strings and bodies
  placed for you; errors throw `ApiError(status, body)`.
- Every successful write sends a `change` event to that user's streams;
  `--emit-openapi=api.json` describes the same routes for other tools.
