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

const LIST_COMMON: &str = r#"
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
    use_case ListTasksUseCase {
        call: TaskInfra.list
    }
"#;

#[test]
fn get_route_with_no_path_params_reads_its_input_from_the_query_string() {
    // Regression: a GET route's non-path input fields (here: limit/offset,
    // list's guessed input) used to be read from a JSON body — but a real
    // HTTP client's fetch()/XMLHttpRequest refuses to send a body on GET at
    // all, making the route unreachable. Must be `Query`, not `Json`.
    let project = generate(&format!(
        r#"{}
        adapter HttpAdapter {{
            http {{
                routes {{
                    GET "/tasks" {{
                        use_case: ListTasksUseCase
                    }}
                }}
            }}
        }}"#,
        LIST_COMMON
    ));

    let route = project.read("src/adapters/http/http_adapter/list_tasks_use_case_get.rs");
    assert!(route.contains("use axum::{Json, extract::State, http::StatusCode, extract::Query};"));
    assert!(route.contains("Query(input): Query<ListTasksUseCaseInput>,"));
    assert!(!route.contains("Json(input)"));
}

#[test]
fn non_get_routes_still_read_their_body_from_json() {
    let project = generate(&format!(
        r#"{}
        adapter HttpAdapter {{
            http {{
                routes {{
                    POST "/tasks" {{
                        use_case: CreateTaskUseCase
                    }}
                }}
            }}
        }}"#,
        COMMON
    ));

    let route = project.read("src/adapters/http/http_adapter/create_task_use_case_post.rs");
    assert!(route.contains("Json(input): Json<CreateTaskUseCaseInput>,"));
    assert!(!route.contains("extract::Query"));
}

#[test]
fn route_file_imports_uuid_and_decimal_for_locally_declared_split_structs() {
    // Regression: a route whose path/body split structs (declared inline in
    // the route file itself, not the crate-level {Name}Input struct) use a
    // Uuid or Decimal field used to compile-fail with "cannot find type
    // Uuid"/"Decimal" — nothing imported them for this file specifically.
    let project = generate(
        r#"type Invoice {
            id: Int,
            owner_id: Uuid,
            amount: Decimal
        }
        infra InvoiceInfra {
            storage {
                psql {
                    entity: Invoice
                }
            }
        }
        use_case UpdateInvoiceUseCase {
            call: InvoiceInfra.update_one
        }
        adapter HttpAdapter {
            http {
                routes {
                    PUT "/invoices/:id" {
                        use_case: UpdateInvoiceUseCase
                    }
                }
            }
        }"#,
    );

    let route = project.read("src/adapters/http/http_adapter/update_invoice_use_case_put.rs");
    assert!(route.contains("use uuid::Uuid;"));
    assert!(route.contains("use rust_decimal::Decimal;"));
    assert!(route.contains("pub id: i64,"));
    assert!(route.contains("pub owner_id: Uuid,"));
    assert!(route.contains("pub amount: Decimal,"));
}
