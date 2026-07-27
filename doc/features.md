# Features

## 1. Type System

What it does:
- Defines domain entities, request, and response shapes.
- Supports built-in scalar types and user-defined types.

How to use:
- Create struct types with fields.
- Use aliases for reusable semantic names.
- Use `export` on types you plan to import from other `.tr` files.

## 2. Import and Reuse

What it does:
- Reuses exported Trust types across modules.

How to use:
- Use `import X from "./module.tr"` for one item.
- Use `import { A, B } from "./module.tr"` for many.

Notes:
- Module path imports are supported.
- Git import syntax is parsed, but code generation currently rejects git imports.

## 3. Storage Infrastructure

### PostgreSQL (`psql`)
What it does:
- Generates storage implementations for CRUD/list operations.

How to use:
- Declare `infra` with `storage { psql { entity: YourEntity } }`.

### gRPC Client Storage (`grpc_client`)
What it does:
- Lets one app call another app's gRPC adapter through storage trait mapping.

How to use:
- In `methods`, map storage operations (`find_one`, `save_one`, etc.) to RPC names.
- Set static target address with `on`.

## 4. Use Cases

What it does:
- Defines application behavior and error mapping.

How to use:
- Deterministic mode: one or more `call` entries.
- AI mode: one `ai` block with `prompt`.
- Optionally provide explicit `input` and `output` types.

## 5. HTTP Adapter

What it does:
- Exposes use cases as REST endpoints.

How to use:
- Define routes per method and path.
- Attach `input`, `output`, `use_case`.
- Add route-level `status` mappings and optional `auth`.

Supported methods:
- GET
- POST
- PUT
- PATCH
- DELETE

## 6. gRPC Adapter

What it does:
- Exposes use cases as gRPC RPC methods.
- Generates `.proto` and server wiring.

How to use:
- Define `grpc { services { rpc ... } on: "..." }` under an adapter.

## 7. App Composition and Multi-App Workspaces

What it does:
- Builds one or many binaries from one Trust source file.

How to use:
- Declare multiple `app` blocks when you need separate runtimes.
- Put different adapters/roles in each app.

## 8. Load Balancer Generation

What it does:
- Generates a round-robin reverse proxy.

How to use:
- Put `load_balancer` inside an `app ... run` block.
- Configure `targets` and `on` bind address.

## 9. AI Use Case Generation

What it does:
- Generates use case function body through an AI provider.

How to use:
- Add `ai { prompt: '...' }` in a use case.
- Run compiler with AI flags or required API key env var.

CLI-related flags:
- `--ai-provider=anthropic`
- `--ai-api-key=...`
- `--ai-model=...`
- `--ai-base-url=...`

## 10. Plugin Extensibility

What it does:
- Dynamically loads compiler plugins (`.so`) that register new top-level keywords and template-driven code generation.

How to use:
- Pass one or more `--plugin=/absolute/path/to/libplugin.so` flags.
- Use the plugin keyword block in your `.tr` source.

## 11. Dev Mode Artifacts

What it does:
- Generates additional development artifacts such as docs pages and helper outputs.

How to use:
- Run with `--dev`.

## 12. Scenarios and E2E Workflows

What it does:
- Demonstrates complete workflows:
  - CRUD HTTP + PSQL
  - gRPC + load balancer
  - AI + plugin
  - multi-app workspace
  - cross-binary gRPC

How to use:
- Run scenario scripts under `scenarios/*/run.sh`.
