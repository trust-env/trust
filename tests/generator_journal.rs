#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

fn tr(dir: &std::path::Path) -> String {
    std::fs::write(
        dir.join("plan.rs"),
        "use crate::*;\npub async fn execute(deps: &impl PlanUseCaseDeps, user: i64, input: PlanRequest) -> Result<PlanResult, PlanUseCaseError> {\n    let _ = (user, input);\n    deps.journal().entry(\"moved\").requires_ack(true);\n    Ok(PlanResult { action_id: None })\n}\n",
    )
    .unwrap();
    format!(
        r#"type Task {{ id: Int, user_id: Int, title: String }}
type Action {{ id: Int, user_id: Int, kind: String, text: String, changes: Json, undone: Bool, at: DateTime, requires_ack: Bool }}
type PlanRequest {{ moves: Int }}
type PlanResult {{ action_id: Option<Int> }}
infra TaskInfra {{ storage {{ psql {{ entity: Task owner: user_id }} }} }}
infra ActionInfra {{ storage {{ psql {{ entity: Action owner: user_id }} }} }}
journal Actions {{ infra: ActionInfra owner: user_id undo: "/actions/:id/toggle" conflict: refuse }}
resource Tasks {{ infra: TaskInfra path: "/tasks" journal: Actions list create }}
use_case PlanUseCase {{
    input: PlanRequest
    output: PlanResult
    journal: Actions
    text: "Plan {{input.moves}}"
    code {{ module: "{plan}" infra: TaskInfra }}
}}
adapter Api {{ http {{ on: "0.0.0.0:8080" auth: required resources {{ Tasks Actions }} routes {{ POST "/plan" {{ use_case: PlanUseCase }} }} }} }}
app Main: run {{ Api }}
"#,
        plan = dir.join("plan.rs").display()
    )
}

#[test]
fn journaled_use_cases_record_rows_in_a_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let create = p.read("main/src/applications/create_task_use_case.rs");
    assert!(create.contains("let journal = crate::TrustJournal::new_run();"), "{create}");
    assert!(create.contains("in_journaled_transaction(&tx, &journal)"), "{create}");
    assert!(create.contains(r#"crate::trust_write_actions(&tx, user, "CreateTaskUseCase", &text, &journal)"#), "{create}");
    // Reads aren't journaled.
    assert!(!p.read("main/src/applications/list_tasks_use_case.rs").contains("TrustJournal"));

    let plan = p.read("main/src/applications/plan_use_case.rs");
    assert!(plan.contains(r#""Plan {input.moves}""#), "{plan}");
    assert!(plan.contains("output.action_id = action_id;"), "{plan}");
    assert!(plan.contains("pub trust_journal: TrustJournal,"), "{plan}");
    assert!(plan.contains("fn journal(&self) -> &TrustJournal"), "{plan}");

    let storage = p.read("main/src/infrastructures/storage/psql/task.rs");
    assert!(storage.contains("self.journal_write(row.id, None, false).await?;"), "{storage}");
    assert!(storage.contains("SELECT to_jsonb(t) FROM tasks t WHERE id = $1"), "{storage}");
}

#[test]
fn the_journal_module_writes_entries_and_toggles_them() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let journal = p.read("main/src/trust_journal.rs");
    assert!(journal.contains("pub async fn trust_write_actions("), "{journal}");
    assert!(journal.contains("pub async fn trust_toggle_actions("), "{journal}");
    assert!(journal.contains(r#""requires_ack" => {"#), "{journal}");
    assert!(journal.contains(r#""tasks" => Some("user_id, title"),"#), "{journal}");
    assert!(journal.contains("changed since this action"), "{journal}");
    // A row already where the toggle would put it is not a conflict.
    assert!(journal.contains("if current.as_ref() == final_target {"), "{journal}");

    let adapter = p.read("main/src/adapters/http/api.rs");
    assert!(adapter.contains(r#".route("/actions/{id}/toggle", post(Self::toggle_actions_use_case_post))"#), "{adapter}");
    let route = p.read("main/src/adapters/http/api/toggle_actions_use_case_post.rs");
    assert!(route.contains("409"), "{route}");
}

#[test]
fn journal_entities_need_the_journal_fields() {
    let dir = tempfile::tempdir().unwrap();
    let err = try_generate(&tr(dir.path()).replace("undone: Bool, ", "")).unwrap_err();
    assert!(err.contains("needs a field 'undone' of type Bool"), "{err}");
}

#[test]
fn unknown_journal_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let err = try_generate(&tr(dir.path()).replace("journal: Actions\n    text", "journal: Nope\n    text")).unwrap_err();
    assert!(err.contains("unknown journal 'Nope'"), "{err}");
}
