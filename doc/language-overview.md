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
- `infra`: define storage backends (`psql` or `grpc_client`).
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
