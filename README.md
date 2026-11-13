# Trust

Trust stands for "Trust is Rust",
or "Trust Rust Services Together."
or "Trust Rust to Serve Things."
or "Trust Rust to Serve Trust Rust Services Together."
or anything else you want it to stand for.

A declarative DSL for wiring Rust services together.  
Does it stand for something? Yes. We'll figure out what later.

It's like Terraform, but for your app layer. And without the 40-minute plan.

---

## Documentation

- [Feature catalog](doc/features.md) — everything Trust can generate, by theme, with examples
- [Tutorial](doc/tutorial.md) — hands-on walkthrough: build, run and call a first service
- [Keyword reference](doc/keywords.md) — every `.tr` construct and its options
- [Recipes](doc/recipes.md) — copy-paste patterns for common needs
- [CLI](doc/cli.md) — compiler flags, dev mode, generated binary commands
- [Conversions](doc/conversions.md) — exactly what each construct compiles to
- [Limitations](doc/limitations.md) — what isn't supported yet
- [Contributing](CONTRIBUTING.md) — running the test suite, adding new language constructs

---

## Why

Wiring Rust microservices by hand is fine. Until it isn't.  
Trust is the thing you write before you regret not writing it.

## Status

The compiler works: `.tr` files generate Rust services that build and run —
HTTP and gRPC APIs, MCP servers for AI agents, Postgres storage with
automatic migrations, S3 blobs, outbound HTTP and OAuth2 clients, calendar
feeds and sync mirrors, an undo journal, scheduled jobs, notifications and
Telegram bots, load balancing, plugins. See the
[feature catalog](doc/features.md). Expect rough edges and breaking changes.

*Built in Rust. Named by committee. Shipped eventually.*

---

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) 1.85+ (edition 2024)

---

## Build

```bash
cargo build
```

For a release build:

```bash
cargo build --release
```

---

## Run

Pass one or more `.tr` files (the entry file of a multi-file project is
enough: its `include`s are followed):

```bash
cargo run -- <file.tr> [file2.tr ...]
```

Or with the release binary:

```bash
./target/release/trust <file.tr> [--out=<dir>] [--dev] [--vendor]
```

The generated Cargo workspace is written to `./out` (wiped first); build it
with `cargo build` there. Flags: [doc/cli.md](doc/cli.md).

### Example

```bash
cargo run -- examples/complete_example.tr --dev
cd out && cargo run
```

`examples/` holds small, complete projects (CRUD over HTTP, gRPC, load
balancer, AI-authored use case, plugins).

---

## DSL overview

A `.tr` project is made of declarations, in any order and across any
number of files:

```tr
config { json_case: "camel" tz: "Europe/Paris" }

type Task {
    id: Int
    user_id: Int
    title: String
    priority: Int(1..4)
    done_at: Option<DateTime>
}
type ReviewRequest { week: Option<Date> }
type Review { done: Int  text: String }

infra TaskInfra {
    storage { psql { entity: Task owner: user_id defaults { priority: 3 } } }
}

resource Tasks { infra: TaskInfra path: "/tasks" list create get patch delete }

use_case WeeklyReviewUseCase {
    input: ReviewRequest
    output: Review
    code { module: "./review.rs" infra: TaskInfra }
}

adapter Api {
    http {
        on: "0.0.0.0:${PORT:-8080}"
        auth: required
        resources { Tasks }
        routes { POST "/review" { use_case: WeeklyReviewUseCase } }
    }
}

schedule Jobs { "0 18 * * sun" => WeeklyReviewUseCase }

app Main: run { Api Jobs }
```

| Keyword | Purpose |
|---|---|
| `type`, `enum` | Data shapes: entities, requests, responses |
| `infra` | Storage (`psql`, `s3`, `grpc_client`) and outside services (`http_client`, `ics_feed`) |
| `use_case` | One operation: storage `call:`s, hand-written `code {}`, `blob:` or `ai {}` |
| `resource`, `journal`, `mirror` | Generated CRUD APIs, an undo log, sync from external sources |
| `adapter` | How use cases are exposed: `http`, `mcp`, `grpc` |
| `schedule`, `notifications`, `bot` | Cron jobs, outbound messages, Telegram bots |
| `app` | What runs together in one binary |
| `include`, `import`, `config` | Multi-file projects and project settings |

Full list: [doc/keywords.md](doc/keywords.md).
