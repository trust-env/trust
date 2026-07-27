#[path = "support/mod.rs"]
mod support;

use support::generate;

const TYPE_AND_INFRA: &str = r#"
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
"#;

const USE_CASE: &str = r#"
    use_case GetTaskUseCase {
        call: TaskInfra.find_one
    }
"#;

const ADAPTER: &str = r#"
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
"#;

#[test]
fn adapter_before_or_after_its_use_case_generates_identical_output() {
    // Regression: adapters used to only see a use_case's resolved shape if
    // the use_case was declared earlier in the file — this pins that the
    // generator's pre-pass makes declaration order irrelevant, by asserting
    // the two orderings produce byte-identical output, not just comparing
    // against one fixed expectation.
    let adapter_first = generate(&format!("{}{}{}", TYPE_AND_INFRA, ADAPTER, USE_CASE));
    let use_case_first = generate(&format!("{}{}{}", TYPE_AND_INFRA, USE_CASE, ADAPTER));

    assert_eq!(
        adapter_first.read("src/adapters/http/http_adapter.rs"),
        use_case_first.read("src/adapters/http/http_adapter.rs"),
    );
    assert_eq!(
        adapter_first.read("src/applications/get_task_use_case.rs"),
        use_case_first.read("src/applications/get_task_use_case.rs"),
    );
}
