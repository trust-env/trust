#[path = "support/mod.rs"]
mod support;

use support::generate;

#[test]
fn use_case_struct_always_derives_clone() {
    // Regression: a use case shared by two adapters needs `.clone()` at each
    // constructor call site in `main.rs` (see below), which only compiles if
    // the use case's own generated struct derives `Clone`.
    let project = generate(
        r#"type Task {
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
        adapter HttpAdapter {
            http {
                on: "0.0.0.0:8080"
                routes {
                    GET "/tasks/:id" {
                        use_case: GetTaskUseCase
                    }
                }
            }
        }
        adapter GrpcAdapter {
            grpc {
                on: "0.0.0.0:50051"
                services {
                    rpc GetTask {
                        use_case: GetTaskUseCase
                    }
                }
            }
        }
        app Main: run {
            HttpAdapter
            GrpcAdapter
        }"#,
    );

    let use_case = project.read("main/src/applications/get_task_use_case.rs");
    assert!(
        use_case.contains("#[derive(Clone)]\npub struct GetTaskUseCase"),
        "use case struct must derive Clone so every adapter referencing it can .clone() its instance:\n{}",
        use_case
    );

    let main_rs = project.read("main/src/main.rs");
    // One constructor call, shared and cloned at each of the two adapters'
    // construction sites.
    assert_eq!(main_rs.matches("get_task_use_case.clone()").count(), 2);
    assert_eq!(main_rs.matches("GetTaskUseCase::new(").count(), 1);
}
