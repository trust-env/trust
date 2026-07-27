#[path = "support/mod.rs"]
mod support;

use support::generate;

#[test]
fn duplicate_infra_blocks_generate_separate_storage_files_without_error() {
    // Regression: two `infra {}` blocks for different entities used to crash
    // on the second one trying to `create_dir` a folder the first already
    // made (fixed by switching to `create_dir_all`, which is idempotent).
    let project = generate(
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
        }"#,
    );

    let task_storage = project.read("src/infrastructures/storage/psql/task.rs");
    let note_storage = project.read("src/infrastructures/storage/psql/note.rs");
    assert!(task_storage.contains("PsqlTaskStorage"));
    assert!(note_storage.contains("PsqlNoteStorage"));
}

#[test]
fn list_method_builds_a_dynamic_where_clause_via_assert_sql_safe() {
    // Regression: `list()` predicates were silently ignored — the generated
    // `list` method must actually build its WHERE clause from
    // `query.predicates` at runtime, not just select everything.
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
        }"#,
    );

    let storage = project.read("src/infrastructures/storage/psql/task.rs");
    assert!(storage.contains("query.predicates"));
    assert!(storage.contains("sqlx::AssertSqlSafe"));
    assert!(storage.contains("WHERE"));
}

#[test]
fn crud_use_case_output_is_a_wrapper_struct_around_the_entity() {
    // Regression: CRUD output used to be flattened instead of wrapping the
    // entity in a single-field struct (`{ task: Task }`, not `Task`'s fields
    // inlined) — this is the shape gRPC's message rendering depends on too.
    let project = generate(
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
        }"#,
    );

    let use_case = project.read("src/applications/create_task_use_case.rs");
    assert!(use_case.contains("pub struct CreateTaskUseCaseOutput {"));
    assert!(use_case.contains("pub task: Task,"));
    assert!(use_case.contains("impl From<Task> for CreateTaskUseCaseOutput"));
}

#[test]
fn list_use_case_output_wraps_a_vec_pluralized_by_entity_name() {
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
        use_case ListTasksUseCase {
            call: TaskInfra.list
        }"#,
    );

    let use_case = project.read("src/applications/list_tasks_use_case.rs");
    assert!(use_case.contains("pub struct ListTasksUseCaseOutput {"));
    assert!(use_case.contains("pub tasks: Vec<Task>,"));
    assert!(use_case.contains("impl From<Vec<Task>> for ListTasksUseCaseOutput"));
}
