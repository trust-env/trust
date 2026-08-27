# Language Overview

Trust is a declarative DSL that generates Rust service code.

## Compilation Flow

1. Parse one or more `.tr` files.
2. Build an AST.
3. Resolve types, infra bindings, and use case signatures.
4. Generate Rust project files from templates.
5. Write generated code under `out/`.

## Main Building Blocks

- `type`: define data models and request/response shapes.
- `infra`: define storage backends (`psql`, `grpc_client`, or `s3` for blob storage).
- `use_case`: define application logic through deterministic calls or AI-authored bodies.
- `adapter`: expose use cases via `http` routes or `grpc` services.
- `app`: choose what runs together in one generated binary.
- `import`: reuse exported Trust types from other `.tr` files.
- `load_balancer`: generate a round-robin reverse proxy.
- Plugin keyword blocks: add custom syntax/features through dynamically loaded compiler plugins.

## Typical Minimal Example

```tr
type User {
    id: Int,
    name: String
}

infra UserInfra {
    storage {
        psql {
            entity: User
        }
    }
}

use_case GetUserUseCase {
    call: UserInfra.find_one
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            GET "/users/:id" {
                use_case: GetUserUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}
```

## Two Things Worth Knowing Immediately

- **Declaration order never matters.** Types, infra, use cases, adapters,
  and apps can reference each other in any order in the file — a two-pass
  resolution registers every name before generating anything.
- **`./out` is fully deleted and rebuilt on every compile.** Never hand-edit
  generated code under `out/` — it will not survive the next run. See
  [cli.md](cli.md) for full CLI/output behavior and
  [conversions.md](conversions.md) for exactly what every construct compiles
  down to.
