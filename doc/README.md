# Trust Documentation

This folder contains feature and language documentation for Trust.

## Contents

- `language-overview.md`: what Trust is, compilation model, and core concepts.
- `keywords.md`: all currently recognized DSL keywords, what they do, and how to use them.
- `features.md`: full feature catalog, including HTTP, gRPC, storage, AI, plugins, and multi-app workflows.
- `recipes.md`: practical copy-paste patterns for common use cases.
- `limitations.md`: current known limitations and unsupported behaviors.

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

Generated Rust code is written under `./out`.
