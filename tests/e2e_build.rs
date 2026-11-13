#[path = "support/mod.rs"]
mod support;

use trust::ai::AiConfig;

/// These build real, generated Rust web-service crates with a genuine
/// `cargo build` subprocess (never `cargo run`/live Postgres — that's a
/// separate, smaller e2e tier). Runtime git dependencies (`common`, `auth`)
/// are patched to this repo's local `runtime/*` crates so nothing here ever
/// needs network access; every crates.io dependency is expected to already be
/// vendored in the local registry cache.
#[test]
fn crud_project_compiles() {
    let project = support::generate(
        r#"type Task {
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
        use_case GetTaskUseCase {
            call: TaskInfra.find_one
        }
        use_case ListTasksUseCase {
            call: TaskInfra.list
        }
        adapter HttpAdapter {
            http {
                on: "0.0.0.0:8080"
                routes {
                    POST "/tasks" {
                        input: CreateTaskRequest
                        output: Task
                        use_case: CreateTaskUseCase
                    }
                    GET "/tasks/:id" {
                        use_case: GetTaskUseCase
                    }
                    GET "/tasks" {
                        use_case: ListTasksUseCase
                    }
                }
            }
        }
        app Main: run {
            HttpAdapter
        }"#,
    );

    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

#[test]
fn grpc_project_compiles() {
    let project = support::generate(
        r#"type Task {
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
        adapter GrpcAdapter {
            grpc {
                on: "0.0.0.0:50051"
                services {
                    rpc GetTask {
                        input: GetTaskRequest
                        output: Task
                        use_case: GetTaskUseCase
                    }
                }
            }
        }
        app Main: run {
            GrpcAdapter
        }"#,
    );

    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

/// A `load_balancer {}`'s proxy is adapter-agnostic (see
/// `templates/file/load_balancer.rs`) — its `targets` are real network
/// addresses of independently deployed processes (e.g. the same binary
/// from a *different* named `app`, deployed once per server), so this
/// fixture needs no local adapter/use_case/infra/type at all to prove a
/// load-balanced project compiles.
#[test]
fn load_balanced_project_compiles() {
    let project = support::generate(
        r#"app Main: run {
            load_balancer LB {
                targets {
                    "10.0.0.1:8080"
                    "10.0.0.2:8080"
                }
                on: "0.0.0.0:9000"
            }
        }"#,
    );

    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

#[test]
fn ai_use_case_project_compiles_with_ai_authored_body_from_a_mock_server() {
    let base_url = support::mock_anthropic_returning("Ok(GreetResponse { greeting: \"test\".to_string() })");
    let ai_config = AiConfig::new_for_testing(base_url);

    let project = support::generate_with_ai(
        r#"type GreetRequest {
            name: String
        }
        type GreetResponse {
            greeting: String
        }
        use_case GreetUseCase {
            input: GreetRequest
            output: GreetResponse
            ai {
                prompt: "return a friendly greeting for the input name"
            }
        }
        adapter HttpAdapter {
            http {
                on: "0.0.0.0:8080"
                routes {
                    POST "/greet" {
                        input: GreetRequest
                        output: GreetResponse
                        use_case: GreetUseCase
                    }
                }
            }
        }
        app Main: run {
            HttpAdapter
        }"#,
        &ai_config,
    );

    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

/// Regression fixture: adapter declared before its use_case in the `.tr`
/// file must compile identically to the reverse order (see
/// `generator_ordering.rs` for the byte-identical-output assertion; this is
/// the compile-level proof the same fix actually produces valid Rust).
#[test]
fn ordering_reversed_project_compiles() {
    let project = support::generate(
        r#"adapter HttpAdapter {
            http {
                on: "0.0.0.0:8080"
                routes {
                    GET "/tasks/:id" {
                        use_case: GetTaskUseCase
                    }
                }
            }
        }
        use_case GetTaskUseCase {
            call: TaskInfra.find_one
        }
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
        app Main: run {
            HttpAdapter
        }"#,
    );

    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

/// The shipped `examples/*.tr` samples are the first thing a new user tries
/// — this is their own proof they keep compiling as the generator evolves,
/// independent of the narrower regression fixtures above.
#[test]
fn shipped_basic_example_wrapped_compiles() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/basic_example_wrapped.tr"))
        .expect("failed to read examples/basic_example_wrapped.tr");

    let project = support::generate(&src);
    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

/// Regression fixture: two `infra {}` blocks for different entities must not
/// only generate without error (see `generator_psql_storage.rs`) but produce
/// a project that actually compiles.
#[test]
fn multi_infra_project_compiles() {
    let project = support::generate(
        r#"type Task {
            id: Int,
            title: String
        }
        type Note {
            id: Int,
            content: String
        }
        infra TaskInfra {
            storage {
                psql {
                    entity: Task
                }
            }
        }
        infra NoteInfra {
            storage {
                psql {
                    entity: Note
                }
            }
        }
        use_case GetTaskUseCase {
            call: TaskInfra.find_one
        }
        use_case GetNoteUseCase {
            call: NoteInfra.find_one
        }
        adapter HttpAdapter {
            http {
                on: "0.0.0.0:8080"
                routes {
                    GET "/tasks/:id" {
                        use_case: GetTaskUseCase
                    }
                    GET "/notes/:id" {
                        use_case: GetNoteUseCase
                    }
                }
            }
        }
        app Main: run {
            HttpAdapter
        }"#,
    );

    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}
