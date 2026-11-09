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
    // The top-level parser dispatches on the construct's leading word, so
    // the use case parser's own precise error reaches the user.
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
    assert!(err.contains("mutually exclusive"), "unexpected error: {}", err);
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

#[test]
fn filters_on_a_non_list_call_is_rejected() {
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
            call: TaskInfra.find_one
            filters {
                id: input.id
            }
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("'filters' is only valid when the primary call is 'list'"), "unexpected error: {}", err);
}

#[test]
fn filters_referencing_an_unknown_entity_field_is_rejected() {
    let err = support::try_generate(
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
        use_case ListTasksUseCase {
            call: TaskInfra.list
            filters {
                owner_id: input.owner_id
            }
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("'filters' references unknown field 'owner_id' on entity 'Task'"), "unexpected error: {}", err);
}

#[test]
fn filters_referencing_a_missing_field_on_an_explicit_input_is_rejected() {
    let err = support::try_generate(
        r#"type Task {
            id: Int,
            title: String
        }
        type ListTasksRequest {
            limit: Int,
            offset: Int
        }
        infra TaskInfra {
            storage {
                psql {
                    entity: Task
                }
            }
        }
        use_case ListTasksUseCase {
            input: ListTasksRequest
            call: TaskInfra.list
            filters {
                title: input.title
            }
        }"#,
    )
    .unwrap_err();
    assert!(
        err.contains("'filters' references 'input.title', but input 'ListTasksRequest' has no such field"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn find_one_on_a_non_int_id_entity_is_rejected() {
    // The shared Storage<T, E> trait's EntityId is fixed to i64 — a Uuid
    // (or Float/Decimal/String) id must be caught at .tr compile time,
    // not left to fail only once the generated crate is actually built.
    let err = support::try_generate(
        r#"type Task {
            id: Uuid,
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
        }"#,
    )
    .unwrap_err();
    assert!(
        err.contains("requires entity 'Task''s 'id' field to be 'Int'"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn save_one_on_a_non_int_id_entity_is_allowed() {
    // save_one/Create never passes a standalone EntityId — the entity's
    // own id field type is irrelevant there, so this must NOT be rejected.
    support::try_generate(
        r#"type Task {
            id: Uuid,
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
        }"#,
    )
    .unwrap();
}

#[test]
fn same_use_case_bound_to_two_routes_sharing_a_method_is_rejected() {
    // The generated route file/function is named {use_case}_{method}, not
    // by path — two routes sharing both collide once the generated crate
    // is built (duplicate module). Catch it at .tr compile time instead.
    let err = support::try_generate(
        r#"type User {
            id: Int,
            role: String
        }
        infra UserInfra {
            storage {
                psql {
                    entity: User
                }
            }
        }
        use_case UpdateUserUseCase {
            call: UserInfra.update_one
        }
        adapter HttpApi {
            http {
                routes {
                    PUT "/users/:id" {
                        use_case: UpdateUserUseCase
                    }
                    PUT "/admin/users/:id/role" {
                        use_case: UpdateUserUseCase
                    }
                }
            }
        }"#,
    )
    .unwrap_err();
    assert!(
        err.contains("both bind use_case 'UpdateUserUseCase'") && err.contains("same HTTP method"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn same_use_case_bound_to_two_routes_with_different_methods_is_allowed() {
    // The collision is on (use_case, method) — different methods on the
    // same use case (an unusual but legal shape) must not false-positive.
    support::try_generate(
        r#"type User {
            id: Int,
            role: String
        }
        infra UserInfra {
            storage {
                psql {
                    entity: User
                }
            }
        }
        use_case GetUserUseCase {
            call: UserInfra.find_one
        }
        adapter HttpApi {
            http {
                routes {
                    GET "/users/:id" {
                        use_case: GetUserUseCase
                    }
                    POST "/users/:id/refresh" {
                        use_case: GetUserUseCase
                    }
                }
            }
        }"#,
    )
    .unwrap();
}

#[test]
fn blob_referencing_unknown_infra_is_rejected() {
    let err = support::try_generate(
        r#"use_case RequestUploadUrlUseCase {
            blob: NoSuchInfra.presigned_put_url
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("isn't an s3 {} infra"), "unexpected error: {}", err);
}

#[test]
fn blob_referencing_a_psql_infra_is_rejected() {
    // `blob:` only binds to `s3 {}` infras — a `psql` infra doesn't
    // implement the blob operations at all, so this must be a clear
    // codegen error, not silently treated as if it were an s3 infra.
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
        use_case RequestUploadUrlUseCase {
            blob: TaskInfra.presigned_put_url
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("isn't an s3 {} infra"), "unexpected error: {}", err);
}

#[test]
fn blob_with_an_unsupported_operation_is_rejected() {
    let err = support::try_generate(
        r#"type MediaObject {
            key: String
        }
        infra MediaInfra {
            storage {
                s3 {
                    bucket: "media"
                    entity: MediaObject
                }
            }
        }
        use_case BadUseCase {
            blob: MediaInfra.list_all
        }"#,
    )
    .unwrap_err();
    assert!(err.contains("Unsupported blob operation 'list_all'"), "unexpected error: {}", err);
}

#[test]
fn blob_use_case_declaring_its_own_input_is_rejected() {
    // Blob use cases always get the fixed `{ key: String }` input `
    // BlobOperation` defines — there's nothing for a declared `input` to
    // override, so declaring one is a hard error rather than silently
    // ignored.
    let err = support::try_generate(
        r#"type MediaObject {
            key: String
        }
        type CustomInput {
            key: String
        }
        infra MediaInfra {
            storage {
                s3 {
                    bucket: "media"
                    entity: MediaObject
                }
            }
        }
        use_case BadUseCase {
            input: CustomInput
            blob: MediaInfra.presigned_put_url
        }"#,
    )
    .unwrap_err();
    assert!(
        err.contains("'blob' use cases have a fixed input/output shape"),
        "unexpected error: {}",
        err
    );
}
