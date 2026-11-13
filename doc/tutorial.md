# Tutorial: your first Trust service

This walkthrough builds a small **notes** HTTP service backed by Postgres:
create a note, read it back, list the notes of a board, delete one. You
write one `.tr` file; Trust generates the whole Rust crate.

Every command and response below was run against the current compiler.

## 0. Prerequisites

- Rust 1.85+ and this repository built (`cargo build --release`).
- Docker (or any Postgres 14+ you can reach).
- Network access to `https://github.com/trust-env/trust.git` on the first
  build: every generated crate depends on Trust's `common` runtime crate
  (`common = { git = "https://github.com/trust-env/trust.git" }`, see
  [runtime/](../runtime/README.md)).

Work in an empty directory: Trust always writes to `./out` and **deletes
it first** on every run.

```bash
mkdir notes && cd notes
```

## 1. Describe the data: `type`

Create `notes.tr`:

```tr
type Note {
    id: Int,
    board_id: Int,
    title: String,
    body: String
}

type CreateNoteRequest {
    board_id: Int,
    title: String,
    body: String
}
```

`Note` will be stored in a table; `CreateNoteRequest` is what clients send.
`Int` becomes `i64`, `String` stays `String` (full list in
[conversions.md](conversions.md)). An `id: Int` field becomes the table's
auto-incremented primary key.

## 2. Say where it lives: `infra`

```tr
infra NoteInfra {
    storage {
        psql {
            entity: Note
        }
    }
}
```

This generates `PsqlNoteStorage` with the 8 storage verbs (`save_one`,
`save_many`, `find_one`, `find_many`, `list`, `update_one`, `delete`,
`delete_many`), plus a migration (`migrations/notes.sql`) that creates the
`notes` table at startup.

## 3. Decide what the service does: `use_case`

```tr
use_case CreateNoteUseCase {
    input: CreateNoteRequest
    output: Note
    call: NoteInfra.save_one
}

use_case GetNoteUseCase {
    call: NoteInfra.find_one
}

use_case ListBoardNotesUseCase {
    call: NoteInfra.list
    filters {
        board_id: input.board_id
    }
}

use_case DeleteNoteUseCase {
    call: NoteInfra.delete
}
```

- Each use case binds to **one storage verb** with `call:`.
- `input`/`output` are optional: when omitted, Trust guesses them from the
  verb (`find_one`/`delete` take an `id`, `list` takes `limit` + `offset`).
- `filters {}` narrows a `list` to rows matching an input field. Trust also
  indexes `board_id` in the migration so the query stays fast.

## 4. Expose it: `adapter`

```tr
adapter NotesApi {
    http {
        on: "0.0.0.0:${PORT}"
        routes {
            POST "/notes" {
                use_case: CreateNoteUseCase
            }
            GET "/notes/:id" {
                use_case: GetNoteUseCase
            }
            GET "/boards/:board_id/notes" {
                use_case: ListBoardNotesUseCase
            }
            DELETE "/notes/:id" {
                use_case: DeleteNoteUseCase
            }
        }
    }
}
```

- `:id` / `:board_id` path segments fill the input field of the same name.
- `POST` bodies are read as JSON; `GET` inputs that aren't path params come
  from the query string.
- `${PORT}` is read from the environment at startup.

## 5. Choose what runs together: `app`

```tr
app NotesService: run {
    NotesApi
}
```

One `app` = one generated binary. A file can declare several apps; each
gets its own crate in the generated workspace.

## 6. Compile

```bash
/path/to/trust/target/release/trust notes.tr --dev
```

`--dev` adds two things: an `out/notes_service/.env` with every variable
the service reads, and a documentation page served at `/app`.

```text
out/
  Cargo.toml                      # workspace (+ optimised release profile)
  notes_service/
    .env
    Cargo.toml
    migrations/notes.sql
    src/
      main.rs                     # wiring: env -> storage -> use cases -> HTTP
      domains/                    # one file per `type`
      infrastructures/storage/psql/note.rs
      applications/               # one file per `use_case`
      adapters/http/notes_api/    # one file per route
```

The generated `.env`:

```bash
NOTE_INFRA_DB_URL=postgres://postgres:postgres@localhost:5432/note
NOTE_INFRA_DB_POOL_SIZE=16
PORT=3000
```

## 7. Run it

Start a database matching the `.env`:

```bash
docker run --rm -d --name notes-pg -p 5432:5432 \
  -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=note postgres:16
```

Then the service:

```bash
cd out/notes_service
cargo run --release
# HTTP Gateway listening on: 0.0.0.0:3000
```

The table (and its index) are created on startup; there is no separate
migration step.

## 8. Call it

```bash
curl -X POST localhost:3000/notes -H 'content-type: application/json' \
  -d '{"board_id": 1, "title": "Courses", "body": "Lait, pain"}'
# {"note":{"id":1,"board_id":1,"title":"Courses","body":"Lait, pain"}}

curl localhost:3000/notes/1
# {"note":{"id":1,"board_id":1,"title":"Courses","body":"Lait, pain"}}

curl 'localhost:3000/boards/1/notes?limit=10&offset=0'
# {"notes":[{"id":1,"board_id":1,"title":"Courses","body":"Lait, pain"}]}

curl -i localhost:3000/notes/42
# HTTP/1.1 404 Not Found

curl -X DELETE localhost:3000/notes/1
# {"note":{"id":1,"board_id":1,"title":"Courses","body":"Lait, pain"}}
```

Things worth knowing:

- Responses wrap the entity in an object named after it (`{"note": ...}`,
  `{"notes": [...]}` for lists) — `envelope: false` on the `http {}` block
  returns the entity itself.
- `limit` and `offset` are optional on list routes (100 and 0 by default);
  a page holds at most 1000 rows and is ordered by `id` unless the use case
  says `order_by:`.
- `DELETE` / `PUT` on an id that doesn't exist answers `404`.
- Open `http://localhost:3000/app` for the generated route documentation
  (`--dev` builds only).

## 9. Where to go next

- Map storage errors to your own error names and HTTP statuses:
  `errors {}` on a `call` and `status {}` on a route — [keywords.md](keywords.md).
- Scope rows to the caller with `owner:` and protect routes with `auth` —
  [keywords.md](keywords.md#owner).
- Expose the same use cases over gRPC, or split them across several apps —
  [features.md](features.md), and the runnable [`scenarios/`](../scenarios/README.md).
- Copy-paste patterns: [recipes.md](recipes.md). Known gaps:
  [limitations.md](limitations.md). Every CLI flag: [cli.md](cli.md).
