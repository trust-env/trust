#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const TR: &str = r#"config { tz: "Europe/Paris" locale: "fr" }
enum Domain { pro perso }
type Suggestion { domain: Domain, due: Option<DateTime>, estimate_min: Int }
type InboxItem { id: Int, user_id: Int, title: String, suggestion: Suggestion, extra: Option<Suggestion> }
type Task { id: Int, user_id: Int, title: String, domain: Domain, due: Option<DateTime>, estimate_min: Int, priority: Int, day: Date, note: String, extra_min: Option<Int> }
type Action { id: Int, user_id: Int, kind: String, text: String, changes: Json, undone: Bool, at: DateTime, requires_ack: Bool }
type SaveTaskResult = Omit<Task, user_id> + { action_id: Option<Int> }
type TaskWithAction = Task + { action_id: Option<Int> }
infra TaskInfra { storage { psql { entity: Task owner: user_id defaults { priority: 3 day: today() } } } }
infra ActionInfra { storage { psql { entity: Action owner: user_id } } }
journal Actions { infra: ActionInfra owner: user_id }
map InboxItem -> Task {
    user_id: user_id
    title: title
    domain: suggestion.domain
    due: suggestion.due
    estimate_min: suggestion.estimate_min
    note: "inbox"
    extra_min: extra.estimate_min
}
use_case TodayUseCase { call: TaskInfra.list filters { day: today() } }
use_case SaveTaskUseCase { call: TaskInfra.save_or_patch output: SaveTaskResult journal: Actions }
use_case CreateTaskUseCase { call: TaskInfra.save_one output: TaskWithAction journal: Actions }
adapter Api { http { on: "0.0.0.0:8080" auth: required routes {
    GET "/today" { use_case: TodayUseCase }
    PUT "/tasks" { use_case: SaveTaskUseCase }
    POST "/tasks" { use_case: CreateTaskUseCase }
} } }
schedule Jobs { "0 7 * * *" => TodayUseCase user: 1 }
app Main: run { Api Jobs }
"#;

#[test]
fn the_app_time_zone_drives_helpers_filters_and_defaults() {
    let p = generate(TR);
    let time = p.read("main/src/trust_time.rs");
    assert!(time.contains(r#""Europe/Paris".parse().expect("time zone checked by trust")"#), "{time}");
    for helper in ["pub fn now_local()", "pub fn today()", "pub fn day_bounds(", "pub fn week_bounds(", "pub fn local(", "pub fn local_to_utc("] {
        assert!(time.contains(helper), "missing {helper}");
    }
    let today = p.read("main/src/applications/today_use_case.rs");
    assert!(today.contains(r#"with_timezone(&"Europe/Paris".parse::<chrono_tz::Tz>()"#), "{today}");
    let storage = p.read("main/src/infrastructures/storage/psql/task.rs");
    assert!(storage.contains("let value = crate::tz::today();"), "{storage}");
    assert!(p.read("main/src/main.rs").contains("Europe/Paris"));
}

#[test]
fn the_locale_adds_french_formatting() {
    let time = generate(TR).read("main/src/trust_time.rs");
    assert!(time.contains(r#""mardi""#), "{time}");
    assert!(time.contains(r#"const OVERDUE: &str = "en retard";"#), "{time}");
    assert!(time.contains("pub fn fmt_date(t: impl LocalDateTime, pattern: &str) -> String"), "{time}");
    assert!(time.contains(r#"format!("{}h{:02}", t.hour(), t.minute())"#), "{time}");
    assert!(time.contains("let word = if n.abs() <= 1 { one } else { many };"), "{time}");
}

#[test]
fn maps_convert_one_type_into_another() {
    let maps = generate(TR).read("main/src/trust_maps.rs");
    assert!(maps.contains("impl From<&InboxItem> for Task {"), "{maps}");
    assert!(maps.contains("domain: from.suggestion.domain.clone(),"), "{maps}");
    assert!(maps.contains("extra_min: from.extra.as_ref().map(|v| v.estimate_min.clone()),"), "{maps}");
    assert!(maps.contains(r#"note: "inbox".to_string(),"#), "{maps}");
    assert!(maps.contains("..Default::default()"), "{maps}");
    assert!(maps.contains("crate::apply_task_defaults(&mut to, &["), "{maps}");
    assert!(maps.contains("pub fn from_inbox_item(from: &InboxItem) -> Self {"), "{maps}");
    assert!(maps.contains("pub fn to_task(&self) -> Task {"), "{maps}");
}

#[test]
fn call_outputs_may_project_the_entity_and_add_the_action_id() {
    let p = generate(TR);
    let save = p.read("main/src/applications/save_task_use_case.rs");
    assert!(save.contains("pub struct SaveTaskUseCaseOutput {"), "{save}");
    assert!(!save.contains("pub user_id: i64,"), "{save}");
    assert!(save.contains("action_id: Default::default(),"), "{save}");
    assert!(save.contains("output.action_id = action_id;"), "{save}");
    let create = p.read("main/src/applications/create_task_use_case.rs");
    assert!(create.contains("pub user_id: i64,"), "{create}");
}

#[test]
fn mistakes_are_errors() {
    let err = try_generate(&TR.replace("Europe/Paris", "Mars/Olympus")).unwrap_err();
    assert!(err.contains("unknown time zone 'Mars/Olympus'"), "{err}");
    let err = try_generate(&TR.replace("locale: \"fr\"", "locale: \"tlh\"")).unwrap_err();
    assert!(err.contains("unknown locale 'tlh'"), "{err}");
    let err = try_generate(&TR.replace("domain: suggestion.domain", "domain: suggestion.nope")).unwrap_err();
    assert!(err.contains("'Suggestion' has no field 'nope'"), "{err}");
    let err = try_generate(&TR.replace("estimate_min: suggestion.estimate_min", "estimate_min: extra.estimate_min")).unwrap_err();
    assert!(err.contains("is optional but 'estimate_min' isn't"), "{err}");
    let err = try_generate(&TR.replace("title: title", "title: estimate_min")).unwrap_err();
    assert!(err.contains("has no field 'estimate_min'"), "{err}");
    let err = try_generate(&TR.replace("+ { action_id: Option<Int> }\ntype TaskWithAction", "+ { nope: Int }\ntype TaskWithAction")).unwrap_err();
    assert!(err.contains("the entity has no field 'nope'"), "{err}");
}
