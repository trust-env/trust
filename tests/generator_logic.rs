#[path = "support/mod.rs"]
mod support;

const APP: &str = r#"type Note { id: Int, user_id: Int, text: String }
type NoteRequest { text: String }
infra NoteInfra { storage { psql { entity: Note owner: user_id } } }
use_case AddNoteUseCase {
    input: NoteRequest
    output: Note
    logic {
        if input.text.trim().is_empty() { fail EmptyText("a note needs text") }
        let sig = sha256(input.text)
        return NoteInfra.create(Note { text: "{input.text} ({sig})", user_id: user })
    }
}
adapter Api {
    http {
        on: "0.0.0.0:8080"
        routes {
            POST "/notes" { use_case: AddNoteUseCase auth: required status { EmptyText -> 422 } }
        }
    }
}
app Main: run { Api }
"#;

#[test]
fn logic_generates_a_code_use_case_with_its_errors_routes_and_crates() {
    let project = support::generate(APP);
    let use_case = project.read("main/src/applications/add_note_use_case.rs");
    assert!(
        use_case.contains("pub struct AddNoteUseCase<S0>"),
        "{use_case}"
    );
    assert!(use_case.contains("    EmptyText(String),"), "{use_case}");
    assert!(
        use_case.contains("trust_row.user_id = user; self.note_infra.save_one(trust_row).await?"),
        "{use_case}"
    );
    let route = project.read("main/src/adapters/http/api/add_note_use_case_post.rs");
    assert!(route.contains("422"), "{route}");
    let cargo = project.read("main/Cargo.toml");
    assert!(cargo.contains("sha2") && cargo.contains("hex"), "{cargo}");
}

#[test]
fn loader_reports_script_errors_at_their_tr_position() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.tr");
    std::fs::write(
        &path,
        APP.replace(
            "let sig = sha256(input.text)",
            "let sig = sha256(input.nope)",
        ),
    )
    .unwrap();
    let asts = trust::loader::load_project(&path, &[]).unwrap();
    let err = trust::generator::desugar::desugar(asts).unwrap_err();
    assert!(err.contains("app.tr:9:31: UseCase 'AddNoteUseCase' logic: 'NoteRequest' has no field 'nope' (fields: text)"), "{err}");
    assert!(
        err.contains("9 |         let sig = sha256(input.nope)"),
        "{err}"
    );
}

#[test]
fn logic_needs_declared_input_and_output() {
    let err = support::try_generate(&APP.replace("    input: NoteRequest\n", "")).unwrap_err();
    assert!(
        err.contains("'logic' requires an explicit 'input'"),
        "{err}"
    );
}
