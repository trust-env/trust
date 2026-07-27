#[path = "support/mod.rs"]
mod support;

use support::generate;

const COMMON: &str = r#"
    type Task {
        id: Int,
        title: String
    }
    type CreateTaskRequest {
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
        input: CreateTaskRequest
        output: Task
        call: TaskInfra.save_one
    }
"#;

#[test]
fn input_colon_and_input_block_forms_generate_identical_output() {
    let colon_form = generate(&format!(
        r#"{}
        adapter HttpAdapter {{
            http {{
                routes {{
                    POST "/tasks" {{
                        input: CreateTaskRequest
                        output: Task
                        use_case: CreateTaskUseCase
                    }}
                }}
            }}
        }}"#,
        COMMON
    ));
    let block_form = generate(&format!(
        r#"{}
        adapter HttpAdapter {{
            http {{
                routes {{
                    POST "/tasks" {{
                        input {{ body: CreateTaskRequest }}
                        output: Task
                        use_case: CreateTaskUseCase
                    }}
                }}
            }}
        }}"#,
        COMMON
    ));

    assert_eq!(
        colon_form.read("src/adapters/http/http_adapter.rs"),
        block_form.read("src/adapters/http/http_adapter.rs"),
    );
}

#[test]
fn http_route_registers_method_path_and_use_case() {
    let project = generate(&format!(
        r#"{}
        adapter HttpAdapter {{
            http {{
                routes {{
                    POST "/tasks" {{
                        input: CreateTaskRequest
                        output: Task
                        use_case: CreateTaskUseCase
                    }}
                }}
            }}
        }}"#,
        COMMON
    ));

    let adapter = project.read("src/adapters/http/http_adapter.rs");
    assert!(adapter.contains("\"/tasks\""));
    assert!(adapter.contains("post"));
    assert!(adapter.contains("create_task_use_case"));
}

#[test]
fn empty_http_adapter_generates_without_error() {
    let project = generate("adapter Empty {}");
    assert!(project.exists("src/adapters/http/empty.rs"));
}
