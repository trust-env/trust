TODO

Nothing pending.

DONE

- My World needs (backend for a personal planning app): List/Json/Date/Time
  /enum types and real Regex validation; filter operators, order_by,
  optional paging; patch_one, upsert_one, unique/index/references, column
  renames and drops; transactions and shared code includes; schedule {}
  with time zones; shared-secret auth, service tokens, SSE events, /health,
  structured logs, camelCase, unwrapped responses; http_client retries,
  text and forms; MCP adapter; --emit-ts / --emit-openapi; --vendor,
  --out, quiet compiler output. See doc/keywords.md and doc/cli.md.

- Documentation folder: `doc/` holds the language and usage docs (tutorial,
  language overview, keywords, features, conversions, CLI, recipes,
  limitations), separate from README.md and CONTRIBUTING.md.

- Test plan against regressions: unit tests (`cargo test --lib`), generator
  integration tests (`tests/generator_*`), end-to-end builds of generated
  crates (`tests/e2e_*`) and runnable `scenarios/`, all run in CI
  (`.github/workflows/tests.yml`), with a `cargo llvm-cov` coverage report
  and a 70% line-coverage floor. Test data is generated per test into temp
  directories that are removed automatically.

- Code generation without `format!`: generated Rust/SQL/HTML text lives in
  `templates/**/*.tera`; Rust only builds the data each template renders.
  Shared pieces (use lines, where clauses, parameter lists, mod
  declarations, method bodies) go through `src/generator/rust_code/fragments.rs`
  and `templates/fragments/`. See CONTRIBUTING.md, "Code generation templates".

- Plugins: designed and implemented — dynamically loaded `cdylib` plugins
  register new keywords, parse their own blocks and ship their own
  templates (`src/plugin/`, `examples/plugins/note_plugin`, `tests/e2e_plugin.rs`).
