# Trust Documentation

This folder contains feature and language documentation for Trust.

## Contents

- [`features.md`](features.md): **start here** — the full feature catalog, by theme (types, storage, use cases, resources, undo journal, sync mirrors, HTTP / MCP / gRPC adapters, time and locale, notifications and bots, apps, tooling), each with an example and a link to its reference.
- [`tutorial.md`](tutorial.md): hands-on walkthrough — build, run and call a first service.
- [`language-overview.md`](language-overview.md): what Trust is, compilation model, and core concepts.
- [`keywords.md`](keywords.md): the reference — every DSL construct, its options and behavior.
- [`conversions.md`](conversions.md): the exhaustive reference for every `.tr` → Rust conversion — types, names, storage traits, error enums, path params, env interpolation, proto, dependencies.
- [`cli.md`](cli.md): full CLI reference — every flag, output/exit behavior, and dev mode.
- [`recipes.md`](recipes.md): complete, copy-paste projects — CRUD, gRPC, auth, S3, then modules, smart creation, per-user settings, undo, tested Rust modules, calendar sync (Outlook, Google), notifications, a Telegram bot, an MCP server for AI agents, French messages at local time, live updates with a typed client.
- [`limitations.md`](limitations.md): current known limitations and unsupported behaviors.

## Scope

The documentation in this folder is based on the implemented compiler behavior in `src/`, `examples/`, `tests/`, and `scenarios/`.

## Quick Start

1. Define `type` declarations.
2. Define one or more `infra` blocks.
3. Define `use_case` blocks.
4. Expose use cases through an `adapter` (`http` or `grpc`).
5. Bind adapters to an `app` with `run`.
6. Compile your `.tr` file with:

```bash
cargo run -- path/to/app.tr
```

Generated Rust code is written under `./out` (or `--out=<dir>`); `cd out &&
cargo run` starts it. Larger projects split into files with `include`, and
use `resource`, `journal`, `mirror`, `schedule`, `mcp`, ... — see
[features.md](features.md).
