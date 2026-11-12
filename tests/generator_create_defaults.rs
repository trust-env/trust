#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const TR: &str = r#"enum Status { todo done }
type Task {
    id: Int
    user_id: Int
    title: String
    status: Status
    priority: Int(1..4)
    budget: Option<Float(0..60)>
    created_at: DateTime
    done_at: Option<DateTime>
}
type Settings { id: Int, user_id: Int, margin_min: Int }
infra TaskInfra {
    storage {
        psql {
            entity: Task
            owner: user_id
            defaults { status: "todo" priority: 3 title: shout(status) }
            stamps { done_at: status == "done" }
        }
    }
}
infra SettingsInfra { storage { psql { entity: Settings } } }
resource Tasks { infra: TaskInfra path: "/tasks" create save }
resource Prefs { infra: SettingsInfra path: "/settings" owner: user_id singleton default { margin_min: 10 } get upsert }
adapter Api { http { on: "0.0.0.0:8080" auth: required resources { Tasks Prefs } } }
app Main: run { Api }
"#;

#[test]
fn ranges_are_validated_when_decoding() {
    let p = generate(TR);
    let validation = p.read("main/src/domains/trust_validation.rs");
    assert!(validation.contains("is above the maximum {}\", value, 4)"), "{validation}");
    assert!(validation.contains("if value > 60.0 {"), "{validation}");
    let task = p.read("main/src/domains/task.rs");
    assert!(task.contains("deserialize_with = \"crate::validate_regex_"), "{task}");
}

#[test]
fn defaults_fill_what_a_creation_leaves_out() {
    let p = generate(TR);
    let storage = p.read("main/src/infrastructures/storage/psql/task.rs");
    assert!(storage.contains("pub fn apply_task_defaults(entity: &mut Task, given: &[&str]) {"), "{storage}");
    assert!(storage.contains("let value = Status::Todo;"), "{storage}");
    assert!(storage.contains("let value = crate::shout(&entity.status);"), "{storage}");
    let create = p.read("main/src/applications/create_task_use_case.rs");
    assert!(create.contains("pub status: Option<Status>,"), "{create}");
    assert!(create.contains("pub created_at: Option<DateTime<Utc>>,"), "{create}");
    assert!(create.contains("crate::apply_task_defaults(&mut entity, &given);"), "{create}");
    assert!(create.contains("created_at: input.created_at.unwrap_or_else(chrono::Utc::now),"), "{create}");
}

#[test]
fn save_or_patch_creates_or_patches() {
    let p = generate(TR);
    let save = p.read("main/src/applications/save_task_use_case.rs");
    assert!(save.contains("match input.id {"), "{save}");
    assert!(save.contains(".patch_one(id, Some((\"user_id\", user)), TaskPatch {"), "{save}");
    // Every non-optional field here has a default (or is a timestamp), so
    // nothing is required.
    assert!(!save.contains("is required to create"), "{save}");
    assert!(save.contains("crate::apply_task_defaults(&mut entity, &given);"), "{save}");
    let route = p.read("main/src/adapters/http/api/save_task_use_case_put.rs");
    assert!(route.contains("422"), "{route}");
}

#[test]
fn stamps_follow_their_condition() {
    let p = generate(TR);
    let storage = p.read("main/src/infrastructures/storage/psql/task.rs");
    assert!(
        storage.contains("done_at = CASE WHEN status IN ('done') THEN COALESCE(done_at, now()) ELSE NULL END"),
        "{storage}"
    );
    assert!(storage.contains("let row = self.stamped(row).await?;"), "{storage}");
}

#[test]
fn singleton_resources_get_or_default_and_upsert() {
    let p = generate(TR);
    let get = p.read("main/src/applications/get_settings_use_case.rs");
    assert!(get.contains("get_or_default(\"user_id\", user)"), "{get}");
    assert!(p.read("main/migrations/settingss.sql").contains("CREATE UNIQUE INDEX IF NOT EXISTS settingss_user_id_key"));
    let storage = p.read("main/src/infrastructures/storage/psql/settings.rs");
    assert!(storage.contains("let value = 10;"), "{storage}");
}

#[test]
fn schema_rule_mistakes_are_errors() {
    let err = try_generate(&TR.replace("stamps { done_at: status == \"done\" }", "stamps { created_at: status == \"done\" }")).unwrap_err();
    assert!(err.contains("must be optional"), "{err}");
    let err = try_generate(&TR.replace("priority: 3 title", "priority: \"high\" title")).unwrap_err();
    assert!(err.contains("doesn't fit field 'priority'"), "{err}");
    let err = try_generate(&TR.replace("Int(1..4)", "Int(4..1)")).unwrap_err();
    assert!(err.contains("is empty"), "{err}");
}
