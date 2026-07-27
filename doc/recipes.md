# Recipes

## Recipe 1: Basic CRUD HTTP Service

```tr
type Task {
    id: Int,
    title: String
}

infra TaskInfra {
    storage {
        psql {
            entity: Task
        }
    }
}

use_case CreateTaskUseCase {
    call: TaskInfra.save_one
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            POST "/tasks" {
                use_case: CreateTaskUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}
```

## Recipe 2: Add gRPC Alongside HTTP

```tr
adapter InternalGrpcApi {
    grpc {
        services {
            rpc CreateTask {
                use_case: CreateTaskUseCase
            }
        }
        on: "0.0.0.0:50051"
    }
}

app Main: run {
    HttpApi
    InternalGrpcApi
}
```

## Recipe 3: Cross-Binary Call via grpc_client

```tr
infra RemoteTaskInfra {
    storage {
        grpc_client {
            service: GrpcWorkerAdapter
            on: "127.0.0.1:15051"
            entity: Task
            methods {
                find_one: GetTask
            }
        }
    }
}

use_case GetRemoteTaskUseCase {
    call: RemoteTaskInfra.find_one
}
```

## Recipe 4: Route-Level Auth + Status Mapping

```tr
DELETE "/tasks/:id" {
    use_case: DeleteTaskUseCase
    auth {
        infra: PermissionInfra
        target: "task"
        action: "delete"
    }
    status {
        InternalError -> 500
        BadRequest -> 400
    }
}
```

## Recipe 5: AI-Authored Use Case

```tr
use_case ValidateEmailUseCase {
    input: ValidateEmailRequest
    output: ValidateEmailResponse
    ai {
        prompt: 'Validate email and return response.'
    }
}
```

Compile with AI settings:

```bash
cargo run -- app.tr --ai-provider=anthropic --ai-api-key=YOUR_KEY
```

## Recipe 6: Plugin Keyword Usage

```bash
cargo run -- app.tr --plugin=/abs/path/libnote_plugin.so
```

Then use the plugin keyword in `.tr`:

```tr
note {
    message: "hello from plugin"
}
```

## Recipe 7: Multi-App Workspace with Cross-Binary gRPC

Two independent binaries in one `.tr` file, wired together over gRPC. This
generates a Cargo workspace with two member crates under `out/`.

```tr
type Task {
    id: Int,
    title: String
}

infra TaskInfra {
    storage {
        psql {
            entity: Task
        }
    }
}

use_case GetTaskUseCase {
    call: TaskInfra.find_one
}

adapter WorkerGrpcApi {
    grpc {
        services {
            rpc GetTask {
                use_case: GetTaskUseCase
            }
        }
        on: "0.0.0.0:15051"
    }
}

app Worker: run {
    WorkerGrpcApi
}

infra RemoteTaskInfra {
    storage {
        grpc_client {
            service: WorkerGrpcApi
            on: "127.0.0.1:15051"
            entity: Task
            methods {
                find_one: GetTask
            }
        }
    }
}

use_case GetRemoteTaskUseCase {
    call: RemoteTaskInfra.find_one
}

adapter PublicHttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            GET "/tasks/:id" {
                use_case: GetRemoteTaskUseCase
            }
        }
    }
}

app Gateway: run {
    PublicHttpApi
}
```

`rpc GetTask` deliberately omits `input:`/`output:` — those fields would
only be checked against `GetTaskUseCase`'s own signature, never define it
(see [conversions.md §10](conversions.md#10-routerpc-inputoutput-are-a-cross-check-not-the-source-of-truth)),
so leaving them off here is idiomatic, not incomplete.

`Worker` and `Gateway` compile to two independent binaries (`out/worker/`,
`out/gateway/`) under one workspace root `Cargo.toml`. `Gateway` never
constructs `Worker`'s adapter directly — it calls it over a real gRPC
connection at compile-known address `127.0.0.1:15051`, so both binaries must
actually be running for the end-to-end path to work. See
`scenarios/05-cross-binary-grpc/` for a fully live version of this pattern
(real Postgres, both binaries spawned as processes, a real HTTP round-trip).

## Recipe 8: Dev Mode (Live Docs + Scaffolded `.env`)

```bash
cargo run -- app.tr --dev
```

For every HTTP adapter, this adds a `GET /app` route serving an HTML docs
page (routes, input/output fields, error/status table), and writes a
starter `.env` into that app's crate root with sniffed defaults for any
`${VAR}` found in `on:` fields, DB URLs for each `psql` infra, and
`JWT_SECRET=changeme` if any route uses `auth {}`. See
[cli.md](cli.md#dev-mode---dev) for the exact defaulting rules — treat the
scaffolded values as local-dev placeholders, not production secrets.
