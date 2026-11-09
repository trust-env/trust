# Contributing

Thanks for your interest in contributing!

Contributions of all kinds are welcome: bug reports, feature requests, documentation improvements, tests, and code. The goal of this guide is to make it easy to understand the project's expectations before opening a pull request.

## Getting started

Clone the repository and make sure the project builds successfully:

```bash
git clone <repository-url>
cd <repository-name>

cargo build
cargo test --lib
```

Running an initial build populates your local Cargo cache, which significantly speeds up later test runs.

## Reporting bugs and requesting features

Please open a GitHub issue for:

* Bug reports
* Feature requests
* Documentation improvements

When reporting a bug, include:

* a minimal reproduction, if possible
* the expected behavior
* the actual behavior
* any relevant logs, screenshots, or generated output

## Development workflow

Before opening a pull request, make sure your changes are formatted and lint clean:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Then run the appropriate test tiers for your changes.

## Running the test suite

The test suite is split into progressively more expensive tiers. Each tier is a standard `cargo test` invocation—there is no custom test runner or `xtask`.

```bash
cargo test --lib                  # lexer, parser and compiler unit tests
cargo test --test 'generator_*'   # generator integration tests
cargo test --test 'e2e_*'         # builds generated Rust projects
```

For most changes:

* parser or lexer changes should pass `--lib`
* code generation changes should also pass `generator_*`
* changes affecting generated projects should additionally pass the e2e suite

## e2e setup

The e2e tests generate real Rust crates and build them with Cargo.

All required crates.io dependencies (`axum`, `sqlx`, `tokio`, `tonic`, `rust_decimal`, `rust-s3`, etc.) are expected to already exist in your local Cargo registry. Running `cargo build` once in this repository is usually sufficient — except `rust_decimal` and `rust-s3`, which the compiler itself doesn't depend on (only generated projects do), so a fixture using `Decimal` or `s3 {}` storage needs one online `cargo build`/`cargo fetch` against a generated project first to seed it into the offline e2e registry cache.

### Private repositories

Generated projects depend on the private BH-World `common` repository.

Clone it into the expected location:

```bash
git clone git@github.com:BH-World/common.git mod/common
```

Future fixtures may also require:

* `mod/auth`
* `mod/user_permission`

The e2e test harness automatically patches Cargo dependencies to these local checkouts. See `LOCAL_GIT_DEPENDENCIES` in `tests/support/mod.rs` for the complete list.

### Database

The current e2e suite does **not** require a running PostgreSQL instance.

Storage generation uses `sqlx::query_as`, which performs runtime validation rather than compile-time SQL checking.

If a future fixture requires a live database, start a disposable instance:

```bash
docker run --rm -d \
  -p 5433:5432 \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=trust_e2e \
  postgres:16
```

Likewise, the current `s3 {}` storage tests (`tests/generator_s3_storage.rs`) only check generated source text, not a live bucket. A future fixture that actually exercises `presigned_put_url`/`presigned_get_url`/`delete` against a real backend can start a disposable MinIO instance:

```bash
docker run --rm -d \
  -p 9000:9000 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  minio/minio server /data
```

Prefer a non-default port if you already have PostgreSQL running locally, and avoid pointing tests at a long-lived development database.

When adding new fixtures, use unique entity names (for example `GrpcDemoUser` instead of `User`) so parallel test runs cannot collide on shared tables.

## Code coverage

Generate a local coverage summary with:

```bash
cargo llvm-cov \
  --lib \
  --test 'generator_*' \
  --summary-only \
  --ignore-filename-regex '(^|/)tests/|(^|/)out/|src/main\.rs'
```

Coverage measures only the compiler source under `src/`.

Generated crates produced during e2e tests are intentionally excluded, as they are temporary build artifacts rather than part of the compiler itself.

CI currently enforces a **70% line coverage** minimum.

Treat an unexpected coverage regression the same way you would treat a failing test: investigate it before merging.

## Code generation templates

Generated-file *text* lives in `templates/*.tera` (rendered via
[Tera](https://keats.github.io/tera/)), not in `format!`/`push_str` chains —
Rust only resolves *data* (field types, generic params, which infra a call
targets, ...) and hands it to a template as a `tera::Context`; the template
decides layout. `src/generator/rust_code/render.rs` bundles every template
into the binary at compile time (`include_str!`), so there's no runtime
dependency on the `templates/` directory being present.

When changing what a generated file looks like, edit the relevant
`templates/**/*.tera` file, not Rust code. When adding a new generated
artifact, add a template under `templates/` (mirroring the writer's path
under `src/generator/rust_code/`) and register it in `render.rs`'s
`TEMPLATES` list and its own test.

Where the line sits in practice:

- **Data, fine in Rust**: identifiers and type names (`format!("{}Trait", name)`,
  `S0`, `Result<Task, PsqlTaskStorageError>`), and Rust string literals of a
  value (`format!("{:?}", value)`).
- **Text, always a template**: anything carrying Rust/SQL/HTML syntax — `use`
  lines, `where` clauses, parameter lists, `mod` declarations, expressions,
  SQL clauses, whole blocks.
- Small pieces shared by many generators have helpers in
  `src/generator/rust_code/fragments.rs` (`where_clause`, `where_block`,
  `type_params`, `params`, `mod_decls`, `indented_body`) and
  `imports.rs` (`use_statement`, `use_group`), each backed by
  `templates/fragments/*.tera`. Reuse them instead of re-joining strings.

One easy-to-miss Tera gotcha: `{%- -%}` trims *all* adjacent whitespace after
a tag, including a following line's leading indentation — not just the
newline. If a loop's next line needs to stay indented, keep the closing tag
and that line's content on the same source line (no trim marker) rather than
trimming across the line break.

## Adding a new `.tr` language construct

Changes to the language grammar should include appropriate coverage across every compilation stage.

A typical contribution includes:

1. A lexer test if new tokens are introduced.
2. Parser tests covering the new AST shape.
3. At least one negative parser test for invalid syntax, when applicable.
4. Tests covering every accepted syntax form if multiple forms produce identical semantics.
5. A generator test validating the produced Rust output — and, if the
   construct needs new generated-file text, a new/updated `templates/*.tera`
   file rather than inline `format!` calls (see "Code generation templates"
   below).
6. An e2e fixture that successfully builds the generated project.
7. A regression test whenever fixing a previously discovered bug.

The goal is not simply to increase coverage, but to ensure every language feature is exercised from parsing through code generation and final compilation.

## Pull requests

Before opening a pull request, verify that:

* the relevant test tiers pass
* formatting and Clippy checks succeed
* new behavior is covered by tests
* documentation is updated when appropriate

Small, focused pull requests are easier to review than large batches of unrelated changes.

Whenever possible, include a brief explanation of the motivation behind the change and any trade-offs considered.
