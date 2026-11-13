#[path = "support/mod.rs"]
mod support;

use support::generate;

const AUTH_TR: &str = r#"type Note {
    id: Int,
    owner_id: Int,
    text: String
}

infra NoteInfra {
    storage {
        psql {
            entity: Note
        }
    }
}

use_case GetNoteUseCase {
    call: NoteInfra.find_one
}

use_case ListNotesUseCase {
    call: NoteInfra.list
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            GET "/notes/:id" {
                use_case: GetNoteUseCase
                auth: required
            }
            GET "/notes" {
                use_case: ListNotesUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}
"#;

#[test]
fn login_only_route_passes_the_jwt_user_id_to_the_use_case() {
    let project = generate(AUTH_TR);
    let route = project.read("main/src/adapters/http/http_api/get_note_use_case_get.rs");

    assert!(route.contains("use auth::UserContext;"), "{route}");
    assert!(route.contains("user_context: UserContext,"), "{route}");
    assert!(
        route.contains(".execute(user_context.user_id, input)"),
        "{route}"
    );
    assert!(
        !route.contains("StorageListQuery"),
        "login-only routes must not do a permission lookup:\n{route}"
    );
    assert!(!route.contains("Missing permission"), "{route}");
}

#[test]
fn routes_without_auth_keep_executing_as_user_zero() {
    let project = generate(AUTH_TR);
    let route = project.read("main/src/adapters/http/http_api/list_notes_use_case_get.rs");

    assert!(!route.contains("UserContext"), "{route}");
    assert!(route.contains(".execute(0, input)"), "{route}");
}

#[test]
fn login_only_route_pulls_in_the_auth_crate_but_no_permission_generic() {
    let project = generate(AUTH_TR);
    let cargo = project.read("main/Cargo.toml");
    assert!(
        cargo.contains(&format!("auth = {{ git = \"{}\" }}", trust::generator::RUNTIME_GIT_URL)),
        "{cargo}"
    );

    let adapter = project.read("main/src/adapters/http/http_api.rs");
    assert!(
        !adapter.contains("A0"),
        "no permission storage generic expected:\n{adapter}"
    );
}

#[test]
fn dev_mode_scaffolds_jwt_secret_for_login_only_routes() {
    let asts = support::parse(AUTH_TR);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("out");
    trust::generator::CodeGen::new(asts)
        .run(
            root.clone(),
            true,
            &trust::ai::AiConfig::new_for_testing("http://127.0.0.1:1"),
        )
        .unwrap();
    let env = std::fs::read_to_string(root.join("main/.env")).unwrap();
    assert!(env.contains("JWT_SECRET="), "{env}");
}
