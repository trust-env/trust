#[path = "support/mod.rs"]
mod support;

use support::generate;

const COMMON: &str = r#"
    type Task {
        id: Int,
        title: String
    }
    type GetTaskRequest {
        id: Int
    }
    infra TaskInfra {
        storage {
            psql {
                entity: Task
            }
        }
    }
    use_case GetTaskUseCase {
        input: GetTaskRequest
        output: Task
        call: TaskInfra.find_one
    }
"#;

#[test]
fn grpc_rpc_registers_service_and_use_case() {
    let project = generate(&format!(
        r#"{}
        adapter GrpcAdapter {{
            grpc {{
                on: "0.0.0.0:50051"
                services {{
                    rpc GetTask {{
                        input: GetTaskRequest
                        output: Task
                        use_case: GetTaskUseCase
                    }}
                }}
            }}
        }}"#,
        COMMON
    ));

    let adapter = project.read("src/adapters/grpc/grpc_adapter.rs");
    assert!(adapter.contains("get_task_use_case"));
    assert!(project.exists("proto/grpc_adapter.proto"));
}

#[test]
fn grpc_proto_output_message_wraps_the_entity_by_name() {
    // Same CRUD-output-wrapper shape as the storage/use-case regression,
    // rendered into the `.proto` file via `render_wrapper_message`.
    let project = generate(&format!(
        r#"{}
        adapter GrpcAdapter {{
            grpc {{
                on: "0.0.0.0:50051"
                services {{
                    rpc GetTask {{
                        input: GetTaskRequest
                        output: Task
                        use_case: GetTaskUseCase
                    }}
                }}
            }}
        }}"#,
        COMMON
    ));

    let proto = project.read("proto/grpc_adapter.proto");
    assert!(proto.contains("message GetTaskUseCaseOutput {"));
    assert!(proto.contains("Task task = 1;"));
}

#[test]
fn empty_grpc_adapter_generates_without_error() {
    let project = generate(
        r#"adapter EmptyGrpc {
            grpc {
                services {}
            }
        }"#,
    );
    assert!(project.exists("src/adapters/grpc/empty_grpc.rs"));
}
