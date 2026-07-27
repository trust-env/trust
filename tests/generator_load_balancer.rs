#[path = "support/mod.rs"]
mod support;

use support::generate;

/// A `load_balancer {}`'s proxy is adapter-agnostic — it forwards to
/// whatever `targets` (real network addresses) are declared, never
/// constructing or knowing anything about what's behind them. This pins
/// that `main.rs` only ever spawns the proxy task, never a local backend.
#[test]
fn load_balancer_proxies_to_declared_targets_without_constructing_a_local_backend() {
    let project = generate(
        r#"app Main: run {
            load_balancer LB {
                targets {
                    "10.0.0.1:8080"
                    "10.0.0.2:8080"
                    "10.0.0.3:8080"
                }
                on: "0.0.0.0:9000"
            }
        }"#,
    );

    assert!(project.exists("main/src/adapters/http/load_balancer.rs"));

    let main_rs = project.read("main/src/main.rs");
    assert!(
        main_rs.contains(
            r#"RoundRobinProxy::new(vec!["10.0.0.1:8080".to_string(), "10.0.0.2:8080".to_string(), "10.0.0.3:8080".to_string()])"#
        ),
        "expected the proxy to be built with the 3 declared target addresses:\n{}",
        main_rs
    );
    assert!(main_rs.contains("load_balancer_router("));
    assert!(main_rs.contains(r#"start_http_server(load_balancer_router(lb_proxy), lb_address)"#));
    // No local backend is ever constructed or spawned for a load balancer.
    assert!(!main_rs.contains("start_http_server(backend.router()"));
}

#[test]
fn load_balancer_can_coexist_with_a_bare_adapter_in_the_same_app() {
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
            GrpcAdapter
            load_balancer LB {
                targets {
                    "10.0.0.1:8080"
                }
                on: "0.0.0.0:9000"
            }
        }"#,
    );

    let main_rs = project.read("main/src/main.rs");
    assert!(main_rs.contains("GrpcAdapter::new("));
    assert!(main_rs.contains("RoundRobinProxy::new(vec![\"10.0.0.1:8080\".to_string()])"));
}
