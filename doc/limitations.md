# Current Limitations

## External Rust Type Reuse

- Directly importing an existing Rust `.rs` type/class into Trust DSL is not supported.
- Trust imports operate on exported Trust types from `.tr` modules.

## Git Imports

- Git-style module import syntax is recognized by the parser.
- Code generation currently rejects git imports ("Git imports are not
  supported yet") — despite this exact form appearing in the top-level
  README's own DSL example. Only local path imports work.

## `grpc_client` Storage Coverage

- Only `find_one` is implemented as a mappable `grpc_client` storage method.
- Mapping any other operation (`save_one`, `save_many`, `find_many`, `list`,
  `update_one`, `delete`, `delete_many`) in `methods { ... }` is accepted by
  the parser but rejected at codegen time.

## gRPC Field Type Coverage

- `rpc`/gRPC message fields only support `String` and `Int` (`i64`).
- `Uuid`, list-shaped (`Vec<...>`) outputs, and nested struct fields are a
  hard **generation-time** error ("gRPC doesn't support field type '...' yet
  — only String and Int fields are supported"). Design any gRPC-exposed type
  with flat scalar fields only.
- gRPC error-to-status mapping is entirely hardcoded (`NotFound` →
  `Status::not_found`, everything else → `Status::internal`) — there is no
  `status {}`-equivalent customization point for `rpc`, unlike HTTP routes.

## HTTP Status Mapping

- `NotFound` and `StorageError` always map to `404`/`500` respectively and
  **cannot be overridden** by a route's `status {}` block, even if you write
  an explicit `NotFound -> 400` entry — the generated code emits the
  hardcoded arms unconditionally.

## Load Balancer Targets

- `load_balancer { targets { ... } }` entries are always static string
  literals. Unlike adapter `on:` and the load balancer's own `on:`, `${VAR}`
  environment interpolation does **not** apply to `targets` — changing the
  backend fleet requires recompiling.

## `Regex<...>` Type Semantics

- `Regex<...>` syntax exists as a declared type form.
- It compiles down to a plain `String` field — the pattern is discarded at
  codegen and no runtime validator is ever generated or enforced.
- The pattern itself is also restricted at the lexer level to a single
  alphanumeric/`_` word — real regex metacharacters break lexing.

## AI Provider Support

- `anthropic` is the only supported provider path in the current
  implementation; any other `--ai-provider` value errors the first time an
  `ai {}` use case needs to call out.
- AI use cases always get a fixed `{NotFound, StorageError(String)}` error
  enum — `errors {}` customization isn't available for `ai` use cases the
  way it is for `call`-based ones.

## Plugin Constraints

- Plugins must match the expected ABI version (currently version `1`) — no
  partial/best-effort compatibility across versions.
- Plugin-generated output file names are restricted to safe, single-segment
  names (no `/`, `\`, or `..`).
- A plugin block is a single opaque top-level construct: it can't introduce
  a new field/expression *type* usable elsewhere in the language, and can't
  make other builtin constructs recognize new syntax.
- There is no plugin discovery/registry — every invocation must pass
  `--plugin=<path>` explicitly; nothing is remembered between runs.

## CLI / Process Behavior

- A missing or unreadable `.tr` file path causes a Rust `panic!` (process
  exit code 101), not the same clean `eprintln!` + `exit(1)` path used for
  every lex/parse/plugin/codegen error.
- A successful compile currently prints the full lexed token stream and the
  full parsed AST to stdout unconditionally — this isn't gated by `--dev` or
  any verbosity flag, and there's no way to suppress it today.
- `./out` (the generation target, currently hardcoded and not
  flag-configurable) is fully deleted and recreated on every run — there is
  no incremental generation and no way to preserve hand-edits.
