#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const TASK_TR: &str = r#"type Task {
    id: Int
    title: String
    status: TaskStatus
    tags: List<String>
    steps: Option<List<Step>>
    meta: Json
    due_on: Option<Date>
    remind_at: Option<Time>
    slug: Regex<slug>
    code: Option<Regex<"^[A-Z]{3}-[0-9]+$">>
}

type Step { label: String, done: Bool }

enum TaskStatus { todo doing done "in-review" }

infra TaskInfra {
    storage {
        psql {
            entity: Task
        }
    }
}

use_case CreateTaskUseCase {
    call: TaskInfra.save_one
}

adapter Api {
    http {
        on: "0.0.0.0:8080"
        routes {
            POST "/tasks" {
                use_case: CreateTaskUseCase
            }
        }
    }
}

app Main: run {
    Api
}
"#;

#[test]
fn list_json_date_and_time_fields_map_to_rust_and_postgres_types() {
    let project = generate(TASK_TR);
    let domain = project.read("main/src/domains/task.rs");
    assert!(domain.contains("use chrono::{NaiveDate, NaiveTime};"), "{domain}");
    assert!(domain.contains("#[sqlx(json)]\n    pub tags: Vec<String>,"), "{domain}");
    assert!(domain.contains("#[sqlx(json(nullable))]\n    pub steps: Option<Vec<Step>>,"), "{domain}");
    assert!(domain.contains("pub meta: serde_json::Value,"), "{domain}");
    assert!(domain.contains("pub due_on: Option<NaiveDate>,"), "{domain}");
    assert!(domain.contains("pub remind_at: Option<NaiveTime>,"), "{domain}");

    let migration = project.read("main/migrations/tasks.sql");
    for column in [
        "status TEXT NOT NULL",
        "tags JSONB NOT NULL",
        "steps JSONB,",
        "meta JSONB NOT NULL",
        "due_on DATE,",
        "remind_at TIME,",
    ] {
        assert!(migration.contains(column), "missing `{column}` in {migration}");
    }

    let storage = project.read("main/src/infrastructures/storage/psql/task.rs");
    assert!(storage.contains(".bind(sqlx::types::Json(data.tags))"), "{storage}");
    assert!(storage.contains(".bind(data.steps.map(sqlx::types::Json))"), "{storage}");
    assert!(storage.contains("col_tags.push(sqlx::types::Json(item.tags));"), "{storage}");

    let cargo = project.read("main/Cargo.toml");
    assert!(cargo.contains("serde_json = \"1\""), "{cargo}");
    assert!(cargo.contains("\"json\""), "{cargo}");
    assert!(cargo.contains("regex = \"1\""), "{cargo}");
}

#[test]
fn enums_serialize_by_wire_value_and_store_as_text() {
    let project = generate(TASK_TR);
    let domain = project.read("main/src/domains/task_status.rs");
    assert!(domain.contains("pub enum TaskStatus {"), "{domain}");
    assert!(domain.contains("#[default]\n    #[serde(rename = \"todo\")]\n    Todo,"), "{domain}");
    assert!(domain.contains("#[serde(rename = \"in-review\")]\n    InReview,"), "{domain}");
    assert!(domain.contains("\"in-review\" => Ok(TaskStatus::InReview),"), "{domain}");
    assert!(domain.contains("impl sqlx::Type<sqlx::Postgres> for TaskStatus"), "{domain}");
    assert!(domain.contains("impl sqlx::postgres::PgHasArrayType for TaskStatus"), "{domain}");
}

#[test]
fn regex_fields_are_validated_when_decoded() {
    let project = generate(TASK_TR);
    let domain = project.read("main/src/domains/task.rs");
    assert!(domain.contains("#[serde(deserialize_with = \"crate::validate_regex_"), "{domain}");
    assert!(domain.contains("#[serde(default, deserialize_with = \"crate::validate_regex_"), "{domain}");

    let validators = project.read("main/src/domains/trust_validation.rs");
    assert!(validators.contains(r#"regex::Regex::new("^[a-z0-9]+(?:-[a-z0-9]+)*$")"#), "{validators}");
    assert!(validators.contains(r#"regex::Regex::new("^[A-Z]{3}-[0-9]+$")"#), "{validators}");

    // The guessed input of a save_one copies the entity's fields, so it is
    // validated too.
    let use_case = project.read("main/src/applications/create_task_use_case.rs");
    assert!(use_case.contains("deserialize_with = \"crate::validate_regex_"), "{use_case}");
}

#[test]
fn unknown_regex_preset_and_invalid_pattern_are_compile_errors() {
    let preset = try_generate(&TASK_TR.replace("Regex<slug>", "Regex<nope>")).unwrap_err();
    assert!(preset.contains("Unknown Regex preset 'nope'"), "{preset}");

    let invalid = try_generate(&TASK_TR.replace(r#"Regex<"^[A-Z]{3}-[0-9]+$">"#, r#"Regex<"([a-z">"#))
        .unwrap_err();
    assert!(invalid.contains("Invalid Regex"), "{invalid}");
}

#[test]
fn types_may_reference_types_declared_later() {
    let project = generate(
        r#"type Order { id: Int, buyer: Buyer }
type Buyer { name: String }
infra OrderInfra { storage { psql { entity: Order } } }
use_case CreateOrderUseCase { call: OrderInfra.save_one }
adapter Api { http { on: "0.0.0.0:8080" routes { POST "/orders" { use_case: CreateOrderUseCase } } } }
app Main: run { Api }
"#,
    );
    let migration = project.read("main/migrations/orders.sql");
    assert!(migration.contains("buyer JSONB NOT NULL"), "{migration}");
}

#[test]
fn duplicate_enum_variant_is_a_parse_error() {
    let err = try_generate("enum Color { red red }").unwrap_err();
    assert!(err.contains("declares 'red' twice"), "{err}");
}
