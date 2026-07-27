# Keywords

This page lists all language keywords currently recognized by Trust.

## Core Structural Keywords

### `type`
Defines a type alias or struct.

```tr
type UserId = Int

type User {
    id: Int,
    name: String
}
```

### `export`
Marks a struct declaration as importable by name from another `.tr` module.

```tr
type PublicUser {
    id: Int,
    name: String
    export PublicUser
}
```

### `import`
Imports exported items from another module.

```tr
import PublicUser from "./shared.tr"
import { User, Role } from "./shared.tr"
```

### `from`
Used in import declarations to specify source module.

## Type Keywords

### `String`
Built-in text type.

### `Uuid`
Built-in UUID type.

### `Int`
Built-in integer type.

### `Regex`
Regex-annotated type form (`Regex<pattern>`).

```tr
type ApiKey {
    id: Int,
    label: Regex<slug>
}
```

### `entity`
Specifies the entity type for storage infra.

## Infrastructure Keywords

### `infra`
Declares an infrastructure service.

### `storage`
Declares storage-backed infra.

### `psql`
PostgreSQL storage backend.

```tr
infra TaskInfra {
    storage {
        psql {
            entity: Task
        }
    }
}
```

### `grpc_client`
gRPC client storage backend for cross-binary calls.

### `service`
Inside `grpc_client`, points to the target gRPC adapter service.

### `methods`
Inside `grpc_client`, maps storage operations to RPC names.

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
```

## Use Case Keywords

### `use_case`
Declares business logic entry points.

### `input`
Sets input type.

### `output`
Sets output type.

### `call`
Binds use case logic to infra operations.

```tr
use_case GetTaskUseCase {
    input: GetTaskRequest
    output: Task
    call: TaskInfra.find_one
}
```

### `errors`
Maps infra/storage errors into use case errors.

```tr
call: {
    name: TaskInfra.find_one
    errors {
        DatabaseError -> InternalError
        ConversionError -> BadRequest("invalid task data")
    }
}
```

### `ai`
Switches use case implementation to AI-authored function body generation.

### `prompt`
Prompt text used by `ai` generation.

```tr
use_case ValidateEmailUseCase {
    input: ValidateEmailRequest
    output: ValidateEmailResponse
    ai {
        prompt: 'Return Ok if email is valid, otherwise an error.'
    }
}
```

## Adapter Keywords

### `adapter`
Declares an adapter boundary.

### `http`
Declares HTTP adapter configuration.

### `routes`
Route list for HTTP adapter.

### `status`
Route-level error-to-HTTP status mapping.

### `body`
Used in wrapped input style (`input { body: Type }`).

### `auth`
Route-level authorization requirement.

```tr
DELETE "/tasks/:id" {
    use_case: DeleteTaskUseCase
    auth {
        infra: PermissionInfra
        target: "task"
        action: "delete"
    }
    status {
        _ -> 500
    }
}
```

### `grpc`
Declares gRPC adapter configuration.

### `services`
gRPC service RPC declaration block.

### `rpc`
Declares one gRPC method endpoint.

```tr
adapter InternalGrpcApi {
    grpc {
        services {
            rpc GetUser {
                input: GetUserRequest
                output: User
                use_case: GetUseCase
            }
        }
        on: "0.0.0.0:50051"
    }
}
```

## App and Runtime Keywords

### `app`
Declares a runnable app unit.

### `run`
App runtime binding block.

```tr
app Main: run {
    HttpAdapter
    InternalGrpcApi
}
```

### `load_balancer`
Declares a round-robin reverse proxy app component.

### `targets`
Load balancer backend address list.

### `on`
Bind/listen address in adapters and load balancer.

```tr
app Gateway: run {
    load_balancer ApiLb {
        targets {
            "10.0.0.1:8080"
            "10.0.0.2:8080"
        }
        on: "0.0.0.0:9000"
    }
}
```
