#[path = "support/mod.rs"]
mod support;

/// One test per known error path a `.tr` file can hit before a project is
/// ever generated — each maps to a specific, named failure mode so a future
/// change that silently swallows one of these (e.g. turns a hard error into
/// a warning, or vice versa) shows up as a red test, not a surprise later.
#[test]
fn unterminated_string_literal_is_rejected() {
    let err = support::try_generate(r#"type X { name: "unterminated }"#).unwrap_err();
    assert!(err.contains("Unterminated string literal"), "unexpected error: {}", err);
}

#[test]
fn unknown_token_is_rejected() {
    let err = support::try_generate("type X { name: String } #").unwrap_err();
    assert!(err.contains("Unexpected character"), "unexpected error: {}", err);
}

#[test]
fn use_case_declaring_both_call_and_ai_is_rejected() {
    // `try_use_case_service` itself produces a precise "cannot declare both
    // 'call' and 'ai'" message (see the parser-level unit test in
    // `src/parser/use_case.rs`), but the top-level driver's `.or_else(...)`
    // chain (`src/parser/driver.rs`'s `try_ast`) only ever surfaces the
    // *last* candidate's error, discarding every earlier one — including
    // this specific one — in favor of the generic `try_app_service`
    // fallback. This pins that real, currently-misleading end-to-end
    // behavior; a fix would need `try_ast` to distinguish "wrong construct"
    // from "right construct, invalid body", which is a bigger change than
    // this test suite is scoped to make.
    let err = support::try_generate(
        r#"type Task {
            id: Int
        }
        infra TaskInfra {
            storage {
                psql {
                    entity: Task
                }
            }
        }
        use_case Bad {
            call: TaskInfra.find_one
            ai {
                prompt: "do something"
            }
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("Failed to parse AppService"), "unexpected error: {}", err);
}

#[test]
fn http_route_referencing_unknown_use_case_is_rejected() {
    let err = support::try_generate(
        r#"adapter HttpAdapter {
            http {
                routes {
                    GET "/x" {
                        use_case: NoSuchUseCase
                    }
                }
            }
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("Unknown use_case 'NoSuchUseCase'"), "unexpected error: {}", err);
}

#[test]
fn use_case_input_referencing_undeclared_type_is_rejected() {
    let err = support::try_generate(
        r#"type Task {
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
            input: NoSuchRequestType
            output: Task
            call: TaskInfra.find_one
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("Undefined type reference: NoSuchRequestType"), "unexpected error: {}", err);
}

#[test]
fn use_case_call_referencing_unknown_infra_is_rejected() {
    let err = support::try_generate(
        r#"use_case GetTaskUseCase {
            call: NoSuchInfra.find_one
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("Unknown infra 'NoSuchInfra'"), "unexpected error: {}", err);
}
