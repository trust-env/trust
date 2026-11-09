#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const BASE: &str = r#"enum TaskStatus { todo doing done }

type Project { id: Int, user_id: Int, name: String }

type Task {
    id: Int
    user_id: Int
    project_id: Option<Int>
    title: String
    status: TaskStatus
    priority: Int
    due: Option<DateTime>
    note: Option<String>
}

type Event {
    id: Int
    calendar: String
    external_id: String
    start_at: DateTime
    task_id: Option<Int>
}

infra ProjectInfra { storage { psql { entity: Project } } }

infra TaskInfra {
    storage {
        psql {
            entity: Task
            index: [status]
            references { project_id: ProjectInfra }
            renamed { estimate: priority }
            dropped: [legacy]
        }
    }
}

infra EventInfra {
    storage {
        psql {
            entity: Event
            unique: [calendar, external_id]
            references { task_id: TaskInfra cascade }
        }
    }
}
"#;

fn project(use_cases: &str, routes: &str) -> String {
    format!(
        "{BASE}\n{use_cases}\nadapter Api {{ http {{ on: \"0.0.0.0:8080\" routes {{ {routes} }} }} }}\napp Main: run {{ Api }}\n"
    )
}

#[test]
fn filter_operators_become_typed_predicates() {
    let p = generate(&project(
        r#"use_case ListUseCase {
            call: TaskInfra.list
            owner: user_id
            filters {
                status in ["todo", "doing"]
                status != "done"
                priority >= input.min_priority
                title contains input.q
                due < now()
            }
        }"#,
        r#"GET "/tasks" { use_case: ListUseCase auth: required }"#,
    ));
    let uc = p.read("main/src/applications/list_use_case.rs");
    for expected in [
        r#"StoragePredicate { field: "status".to_string(), operator: StoragePredicateOperator::In, value: "todo,doing".to_string() }"#,
        r#"StoragePredicate { field: "status".to_string(), operator: StoragePredicateOperator::NotEq, value: "done".to_string() }"#,
        r#"StoragePredicate { field: "priority".to_string(), operator: StoragePredicateOperator::Gte, value: input.min_priority.to_string() }"#,
        r#"StoragePredicate { field: "title".to_string(), operator: StoragePredicateOperator::ILike, value: format!("%{}%", input.q) }"#,
        r#"StoragePredicate { field: "due".to_string(), operator: StoragePredicateOperator::Lt, value: chrono::Utc::now().to_rfc3339() }"#,
        r#"StoragePredicate { field: "user_id".to_string(), operator: StoragePredicateOperator::Eq, value: user.to_string() }"#,
    ] {
        assert!(uc.contains(expected), "missing `{expected}` in\n{uc}");
    }
    // Guessed input: optional paging, `contains` as text.
    assert!(uc.contains("pub limit: Option<i64>,"), "{uc}");
    assert!(uc.contains("pub q: String,"), "{uc}");
    assert!(uc.contains("limit: input.limit.unwrap_or(DEFAULT_LIST_LIMIT),"), "{uc}");
}

#[test]
fn optional_input_fields_only_filter_when_set() {
    let p = generate(&project(
        r#"type TaskQuery { project_id: Option<Int> }
        use_case ListUseCase {
            input: TaskQuery
            call: TaskInfra.list
            filters { project_id: input.project_id }
        }"#,
        r#"GET "/tasks" { use_case: ListUseCase }"#,
    ));
    let uc = p.read("main/src/applications/list_use_case.rs");
    assert!(uc.contains("if let Some(value) = &input.project_id {"), "{uc}");
    assert!(uc.contains("value: value.to_string()"), "{uc}");
    // No limit/offset on the explicit input: defaults.
    assert!(uc.contains("limit: DEFAULT_LIST_LIMIT,"), "{uc}");
    assert!(uc.contains("offset: 0,"), "{uc}");
}

#[test]
fn order_by_calls_list_sorted_and_is_indexed() {
    let p = generate(&project(
        r#"use_case ListUseCase {
            call: TaskInfra.list
            filters { status: input.status }
            order_by: priority desc, due
        }"#,
        r#"GET "/tasks" { use_case: ListUseCase }"#,
    ));
    let uc = p.read("main/src/applications/list_use_case.rs");
    assert!(uc.contains(".list_sorted("), "{uc}");
    assert!(uc.contains(r#"&[("priority", true), ("due", false)]"#), "{uc}");
    let migration = p.read("main/migrations/tasks.sql");
    assert!(migration.contains("ON tasks (status, priority, id);"), "{migration}");
}

#[test]
fn unknown_order_by_field_is_rejected() {
    let err = try_generate(&project(
        "use_case ListUseCase { call: TaskInfra.list order_by: nope }",
        r#"GET "/tasks" { use_case: ListUseCase }"#,
    ))
    .unwrap_err();
    assert!(err.contains("'order_by' references unknown field 'nope'"), "{err}");
}

#[test]
fn patch_one_takes_optional_fields_and_can_clear_nullable_ones() {
    let p = generate(&project(
        "use_case PatchUseCase { call: TaskInfra.patch_one owner: user_id }",
        r#"PATCH "/tasks/:id" { use_case: PatchUseCase auth: required }"#,
    ));
    let uc = p.read("main/src/applications/patch_use_case.rs");
    assert!(uc.contains("pub title: Option<String>,"), "{uc}");
    assert!(
        uc.contains("#[serde(default, deserialize_with = \"crate::trust_double_option\")]\n    pub note: Option<Option<String>>,"),
        "{uc}"
    );
    assert!(!uc.contains("pub user_id"), "the owner is never patchable:\n{uc}");
    assert!(uc.contains(r#".patch_one(input.id, Some(("user_id", user)), TaskPatch {"#), "{uc}");
    assert!(uc.contains("user_id: None,"), "{uc}");

    // The split path/body structs of the route keep the decoding rule.
    let adapter = std::fs::read_dir(p.root.join("main/src/adapters/http/api"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect::<String>();
    assert!(adapter.contains("crate::trust_double_option"), "{adapter}");
    assert!(p.read("main/src/domains/trust_validation.rs").contains("pub fn trust_double_option"));
}

#[test]
fn upsert_one_needs_a_unique_key() {
    let p = generate(&project(
        "use_case UpsertUseCase { call: EventInfra.upsert_one }",
        r#"PUT "/events" { use_case: UpsertUseCase }"#,
    ));
    let storage = p.read("main/src/infrastructures/storage/psql/event.rs");
    assert!(
        storage.contains("ON CONFLICT (calendar, external_id) DO UPDATE SET start_at = EXCLUDED.start_at, task_id = EXCLUDED.task_id"),
        "{storage}"
    );
    let uc = p.read("main/src/applications/upsert_use_case.rs");
    assert!(uc.contains(".upsert_one(Event::from(input), None)"), "{uc}");

    let err = try_generate(&project(
        "use_case UpsertUseCase { call: TaskInfra.upsert_one }",
        r#"PUT "/tasks" { use_case: UpsertUseCase }"#,
    ))
    .unwrap_err();
    assert!(err.contains("'upsert_one' needs a unique key"), "{err}");
}

#[test]
fn schema_options_render_into_migrations() {
    let p = generate(&project(
        "use_case GetUseCase { call: EventInfra.find_one }",
        r#"GET "/events/:id" { use_case: GetUseCase }"#,
    ));
    let tasks = p.read("main/migrations/tasks.sql");
    assert!(tasks.contains("ALTER TABLE tasks RENAME COLUMN estimate TO priority;"), "{tasks}");
    assert!(
        tasks.find("RENAME COLUMN").unwrap() < tasks.find("ADD COLUMN IF NOT EXISTS priority").unwrap(),
        "renames must run before columns are added:\n{tasks}"
    );
    assert!(tasks.contains("ALTER TABLE tasks DROP COLUMN IF EXISTS legacy;"), "{tasks}");
    assert!(tasks.contains("CREATE INDEX IF NOT EXISTS tasks_status_id_idx ON tasks (status, id);"), "{tasks}");

    let events = p.read("main/migrations/events.sql");
    assert!(
        events.contains("CREATE UNIQUE INDEX IF NOT EXISTS events_calendar_external_id_key ON events (calendar, external_id);"),
        "{events}"
    );
    let fks = p.read("main/migrations/events.fk.sql");
    assert!(
        fks.contains("ADD CONSTRAINT events_task_id_fkey FOREIGN KEY (task_id) REFERENCES tasks (id) ON DELETE CASCADE;"),
        "{fks}"
    );
    // The referenced tables are connected and migrated too, and foreign
    // keys are added once every table exists.
    let main = p.read("main/src/main.rs");
    let fk_at = main.find("events.fk.sql").expect("fk migration runs");
    assert!(main.find("tasks.sql").unwrap() < fk_at, "{main}");
    assert!(main.find("projects.sql").unwrap() < fk_at, "{main}");
}

#[test]
fn schema_option_mistakes_are_compile_errors() {
    // Event references Task, so both are generated.
    let base = || {
        project(
            "use_case GetUseCase { call: EventInfra.find_one }",
            r#"GET "/events/:id" { use_case: GetUseCase }"#,
        )
    };
    let renamed_still_field = try_generate(&base().replace("renamed { estimate: priority }", "renamed { title: priority }"))
        .unwrap_err();
    assert!(renamed_still_field.contains("'title' is still a field"), "{renamed_still_field}");

    let set_null_not_nullable = try_generate(
        &base().replace("references { project_id: ProjectInfra }", "references { priority: ProjectInfra set_null }"),
    )
    .unwrap_err();
    assert!(set_null_not_nullable.contains("can't use set_null"), "{set_null_not_nullable}");
}

#[test]
fn missing_rows_map_to_not_found() {
    let p = generate(&project(
        "use_case DeleteUseCase { call: TaskInfra.delete }",
        r#"DELETE "/tasks/:id" { use_case: DeleteUseCase }"#,
    ));
    let storage = p.read("main/src/infrastructures/storage/psql/task.rs");
    assert!(storage.contains("sqlx::Error::RowNotFound => PsqlTaskStorageError::NotFound,"), "{storage}");
    let uc = p.read("main/src/applications/delete_use_case.rs");
    assert!(uc.contains("PsqlTaskStorageError::NotFound => DeleteUseCaseError::NotFound,"), "{uc}");
}

#[test]
fn filter_syntax_errors_are_clear() {
    let err = try_generate(&project(
        "use_case ListUseCase { call: TaskInfra.list filters { status in \"todo\" } }",
        r#"GET "/tasks" { use_case: ListUseCase }"#,
    ))
    .unwrap_err();
    assert!(err.contains("expected a [list] or input.<field>"), "{err}");
}
