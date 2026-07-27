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
