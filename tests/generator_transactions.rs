#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

fn tr(dir: &std::path::Path, transaction: &str) -> String {
    std::fs::write(dir.join("accept.rs"), "Ok(AcceptResult { task_id: score(1) })\n").unwrap();
    std::fs::write(dir.join("score.rs"), "Ok(AcceptResult { task_id: score(2) })\n").unwrap();
    std::fs::write(dir.join("shared.rs"), "pub fn score(x: i64) -> i64 { x * 2 }\n").unwrap();
    let path = |name: &str| dir.join(name).display().to_string();
    format!(
        r#"type InboxItem {{ id: Int, title: String }}
type Task {{ id: Int, title: String }}
type AcceptRequest {{ id: Int }}
type AcceptResult {{ task_id: Int }}
type RenameRequest {{ id: Int, title: String }}

infra InboxInfra {{ storage {{ psql {{ entity: InboxItem }} }} }}
infra TaskInfra {{ storage {{ psql {{ entity: Task }} }} }}

use_case AcceptUseCase {{
    input: AcceptRequest
    output: AcceptResult
    {transaction}
    code {{
        file: "{accept}"
        include: "{shared}"
        infra: InboxInfra
        infra: TaskInfra
    }}
}}

use_case ScoreUseCase {{
    input: AcceptRequest
    output: AcceptResult
    code {{
        file: "{score}"
        include: "{shared}"
    }}
}}

use_case RecordUseCase {{
    input: RenameRequest
    transaction: true
    call: TaskInfra.save_one
    call: InboxInfra.update_one
}}

adapter Api {{
    http {{
        on: "0.0.0.0:8080"
        routes {{
            POST "/accept" {{ use_case: AcceptUseCase }}
            POST "/score" {{ use_case: ScoreUseCase }}
            POST "/record" {{ use_case: RecordUseCase }}
        }}
    }}
}}
app Main: run {{ Api }}
"#,
        accept = path("accept.rs"),
        score = path("score.rs"),
        shared = path("shared.rs"),
    )
}

#[test]
fn includes_are_copied_once_and_exported_at_the_crate_root() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path(), ""));
    let shared = p.read("main/src/shared/shared.rs");
    assert!(shared.contains("pub fn score(x: i64) -> i64 { x * 2 }"), "{shared}");
    assert_eq!(p.read("main/src/shared.rs"), "mod shared;\n\npub use shared::*;\n");
    assert!(p.read("main/src/lib.rs").contains("mod shared;"));
}

#[test]
fn transactional_code_use_case_runs_its_body_in_one_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path(), "transaction: true"));
    let uc = p.read("main/src/applications/accept_use_case.rs");
    assert!(uc.contains("S0: InboxItemStorage + TrustTransactional,"), "{uc}");
    assert!(uc.contains("let tx = self.inbox_infra.begin().await"), "{uc}");
    assert!(uc.contains("inbox_infra: self.inbox_infra.in_transaction(&tx),"), "{uc}");
    assert!(uc.contains("task_infra: self.task_infra.in_transaction(&tx),"), "{uc}");
    assert!(uc.contains("crate::commit_transaction(&tx)"), "{uc}");
    assert!(uc.contains("let _ = crate::rollback_transaction(&tx).await;"), "{uc}");
    assert!(uc.contains("async fn run(&self, user: i64, input: AcceptRequest)"), "{uc}");
}

#[test]
fn call_use_case_with_transaction_wraps_every_call() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path(), ""));
    let uc = p.read("main/src/applications/record_use_case.rs");
    assert!(uc.contains("S0: TaskStorage + TrustTransactional,"), "{uc}");
    assert!(uc.contains("S1: InboxItemStorage + TrustTransactional,"), "{uc}");
    assert!(uc.contains("let tx = self.task_infra.begin().await"), "{uc}");

    let tx = p.read("main/src/infrastructures/storage/psql/trust_tx.rs");
    assert!(tx.contains("pub type TrustTx"), "{tx}");
    assert!(tx.contains("pub async fn commit_transaction"), "{tx}");
}

#[test]
fn transaction_needs_a_psql_infra() {
    let dir = tempfile::tempdir().unwrap();
    let src = tr(dir.path(), "").replace(
        "    code {\n        file: \"",
        "    transaction: true\n    code {\n        file: \"",
    );
    // ScoreUseCase (no infra) now also asks for a transaction.
    let err = try_generate(&src).unwrap_err();
    assert!(err.contains("'transaction: true' needs at least one psql infra"), "{err}");
}
