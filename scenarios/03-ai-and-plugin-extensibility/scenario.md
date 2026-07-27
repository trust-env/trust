# Scenario 3 — AI use case + plugin extensibility

## Goal

Prove Trust's two most dynamic extension points work correctly, both
individually and together in one generated project:

- **`ai {}` use cases** — a use case whose `execute()` body is authored by
  an LLM at compile time rather than a deterministic CRUD template, bound
  to a `psql` infra so it gets a storage-backed struct (`S0: DocStorage`)
  like any other infra-bound use case.
- **`--plugin`** — a `.so` loaded via `dlopen` at compile time that
  registers a brand-new top-level keyword (`note {}`, from
  `examples/plugins/note_plugin`) with its own parser and its own Tera
  template, and gets wired into the generated project's module tree.

Because a real LLM call would make this script non-deterministic and
network-dependent, the script stands up a one-shot local mock server that
answers exactly like the Anthropic API would, so the whole run is offline
and reproducible.

## What `app.tr` declares

- `Doc` / `SummarizeRequest` / `SummarizeResponse` — plain data types.
- `DocInfra` — `psql` storage for `Doc`.
- `SummarizeUseCase` — an `ai { infra: DocInfra, prompt: "..." }` use case
  (no deterministic `call:` — the compiler asks an LLM to write the body).
- `HttpAdapter` — exposes it as `POST /summarize`.
- `note { message: "..." }` — a plugin-defined construct, unrelated to any
  built-in keyword, generating its own file via its own template.

## How to run it

```bash
./scenarios/03-ai-and-plugin-extensibility/run.sh
```

The script:

1. Builds the `trust` CLI.
2. Builds `examples/plugins/note_plugin` as a real `cdylib` (`.so`).
3. Starts a one-shot local HTTP server that replies to exactly one request
   the same shape the real Anthropic API would, always returning the fixed
   body `Ok(SummarizeResponse { summary: input.text })` — self-terminates
   after 20s if nothing ever connects, so a hung run can't leave a stray
   background process.
4. Runs `trust` against `app.tr` with `--plugin=<path to the .so>`,
   `--ai-base-url=<mock server>` and `--ai-api-key=test-key`.
5. Asserts every expected generated file exists (the ai use case, its HTTP
   route, and the plugin's `main/src/plugins/note.rs` + `main/src/plugins.rs`
   wiring) inside the `main` app's generated crate.
6. Asserts the mocked LLM response was spliced into the use case body
   verbatim, and that the plugin's own template rendered the `note {}`
   block's `message` field correctly.
7. Patches local git dependencies and runs a real `cargo build --offline`
   against the generated project — proving both the AI-authored body and
   the plugin's generated file compile as part of one crate.

Exits non-zero on the first failure; the mock server and scratch directory
are cleaned up on exit either way.
