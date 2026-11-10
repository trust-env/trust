#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

fn tr(dir: &std::path::Path) -> String {
    std::fs::write(
        dir.join("plan.rs"),
        "use crate::*;\npub async fn execute(deps: &impl PlanUseCaseDeps, user: i64, input: PlanRequest) -> Result<PlanResult, PlanUseCaseError> {\n    let tasks = deps.task_infra().query(Query::new().eq(\"user_id\", user)).await?;\n    Ok(PlanResult { planned: tasks.len() as i64, sent: input.send })\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("replan.rs"), "self.plan_use_case.execute(user, PlanRequest { send: false }).await.map_err(Into::into)\n").unwrap();
    format!(
        r#"enum ActionPayload {{
    acceptInbox {{ inbox_id: Int task_id: Int }}
    reset
}}
type Task {{ id: Int, user_id: Int, title: String }}
type Action {{ id: Int, user_id: Int, payload: ActionPayload }}
type PlanRequest {{ send: Bool }}
type PlanResult {{ planned: Int, sent: Bool }}
type Empty {{}}
infra TaskInfra {{ storage {{ psql {{ entity: Task }} }} }}
infra ActionInfra {{ storage {{ psql {{ entity: Action }} }} }}
infra_set Everything {{ TaskInfra ActionInfra }}
infra Feed {{ http_client {{ base_url: "https://example.com" }} }}
resource Actions {{ infra: ActionInfra path: "/actions" name: Act create }}
use_case PlanUseCase {{ input: PlanRequest output: PlanResult code {{ module: "{plan}" infra: Everything }} }}
use_case ReplanUseCase {{ input: Empty output: PlanResult code {{ file: "{replan}" infra: [TaskInfra, Feed] use_case: PlanUseCase }} }}
schedule Jobs {{ "0 7 * * *" => PlanUseCase {{ send: true }} }}
adapter Api {{ http {{ on: "0.0.0.0:8080" resources {{ Actions }} routes {{ POST "/replan" {{ use_case: ReplanUseCase }} }} }} }}
app Main: run {{ Api Jobs }}
"#,
        plan = dir.join("plan.rs").display(),
        replan = dir.join("replan.rs").display(),
    )
}

#[test]
fn infra_sets_and_lists_expand() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let plan = p.read("main/src/applications/plan_use_case.rs");
    assert!(plan.contains("pub task_infra: S0,") && plan.contains("pub action_infra: S1,"), "{plan}");
    let replan = p.read("main/src/applications/replan_use_case.rs");
    assert!(replan.contains("pub feed: FeedClient,"), "{replan}");
}

#[test]
fn module_files_are_real_modules_with_a_deps_trait() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let plan = p.read("main/src/applications/plan_use_case.rs");
    assert!(plan.contains("logic::execute(self, user, input).await"), "{plan}");
    assert!(plan.contains("pub trait PlanUseCaseDeps {"), "{plan}");
    assert!(plan.contains("fn task_infra(&self) -> &(impl TaskStorage + TrustTransactional);"), "{plan}");
    assert!(plan.contains("pub mod logic;"), "{plan}");
    let logic = p.read("main/src/applications/plan_use_case/logic.rs");
    assert!(logic.contains("pub async fn execute(deps: &impl PlanUseCaseDeps"), "{logic}");
}

#[test]
fn code_use_cases_can_run_other_use_cases() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let replan = p.read("main/src/applications/replan_use_case.rs");
    assert!(replan.contains("U0: PlanUseCaseTrait + Clone + Send + Sync,"), "{replan}");
    assert!(replan.contains("impl From<PlanUseCaseError> for ReplanUseCaseError"), "{replan}");
    // Built after the use case it runs.
    let main = p.read("main/src/main.rs");
    let plan_at = main.find("let plan_use_case = PlanUseCase::new(").unwrap();
    let replan_at = main.find("let replan_use_case = ReplanUseCase::new(").unwrap();
    assert!(plan_at < replan_at, "{main}");
    assert!(main.contains("plan_use_case.clone())"), "{main}");
}

#[test]
fn use_case_cycles_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let src = tr(dir.path()).replace(
        r#"code { module: ""#,
        r#"code { use_case: ReplanUseCase module: ""#,
    );
    let err = try_generate(&src).unwrap_err();
    assert!(err.contains("cycle"), "{err}");
}

#[test]
fn schedules_take_literal_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let main = p.read("main/src/main.rs");
    assert!(main.contains(r#"serde_json::to_value(PlanRequest::default())"#), "{main}");
    assert!(main.contains(r#""{\"send\":true}""#), "{main}");

    let err = try_generate(&tr(dir.path()).replace("{ send: true }", "{ nope: true }")).unwrap_err();
    assert!(err.contains("has no input field 'nope'"), "{err}");
}

#[test]
fn tagged_enums_serialize_with_a_type_field() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let payload = p.read("main/src/domains/action_payload.rs");
    assert!(payload.contains(r#"#[serde(tag = "type")]"#), "{payload}");
    assert!(payload.contains("AcceptInbox {\n        inbox_id: i64,"), "{payload}");
    assert!(payload.contains("    Reset,"), "{payload}");
    assert!(p.read("main/migrations/actions.sql").contains("payload JSONB NOT NULL"));
    // `name:` picks the singular of generated names.
    assert!(p.exists("main/src/applications/create_act_use_case.rs"));
}

#[test]
fn storages_offer_query_and_find_one_owned() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let storage = p.read("main/src/infrastructures/storage/psql/task.rs");
    assert!(storage.contains("fn query(&self, query: Query)"), "{storage}");
    assert!(storage.contains("fn find_one_owned(&self, id: EntityId"), "{storage}");
    assert!(p.read("main/src/trust_query.rs").contains("pub fn is_in<"));
}
