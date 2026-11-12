#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const TR: &str = r#"config { deps { "chrono-tz": "0.10" } }
database Main { url: "${MY_DB:-postgres://localhost/app}" pool: 4 }
type Task { id: Int, user_id: Int, title: String, note: Option<String> }
type NewTask = Omit<Task, id, user_id>
type TaskDraft = Partial<Pick<Task, title, note>> + { tags: List<String> }
infra TaskInfra { storage { psql { entity: Task database: Main owner: user_id } } }
resource Tasks {
    infra: TaskInfra
    path: "/tasks"
    list { order_by: title }
    create
    patch
    delete
}
use_case PublicTasksUseCase { call: TaskInfra.list owner: none }
use_case CreateFromDraftUseCase { input: NewTask call: TaskInfra.save_one }
infra Hook { http_client { base_url: "${HOOK_URL}" } }
adapter Api {
    http {
        on: "0.0.0.0:${PORT:-8080}"
        auth: required
        resources { Tasks }
        routes {
            GET "/public/tasks" { use_case: PublicTasksUseCase auth: none }
            POST "/drafts" { use_case: CreateFromDraftUseCase }
        }
    }
}
app Main: run { Api }
"#;

fn route(p: &support::GeneratedProject, name: &str) -> String {
    p.read(&format!("main/src/adapters/http/api/{}.rs", name))
}

#[test]
fn resource_expands_to_use_cases_and_routes_with_the_adapter_auth() {
    let p = generate(TR);
    let adapter = p.read("main/src/adapters/http/api.rs");
    for line in [
        r#".route("/tasks", get(Self::list_tasks_use_case_get))"#,
        r#".route("/tasks", post(Self::create_task_use_case_post))"#,
        r#".route("/tasks/{id}", patch(Self::patch_task_use_case_patch))"#,
        r#".route("/tasks/{id}", delete(Self::delete_task_use_case_delete))"#,
    ] {
        assert!(adapter.contains(line), "missing {line}\n{adapter}");
    }
    assert!(route(&p, "list_tasks_use_case_get").contains("user_context: UserContext,"));
    assert!(p.read("main/src/applications/list_tasks_use_case.rs").contains(r#"&[("title", false)]"#));
}

#[test]
fn auth_none_and_owner_none_opt_out_of_the_defaults() {
    let p = generate(TR);
    assert!(!route(&p, "public_tasks_use_case_get").contains("UserContext"));
    let public = p.read("main/src/applications/public_tasks_use_case.rs");
    assert!(!public.contains("\"user_id\""), "{public}");
    // The infra's owner applies to the other use cases.
    let create = p.read("main/src/applications/create_task_use_case.rs");
    assert!(create.contains("entity.user_id = user;"), "{create}");
}

#[test]
fn create_input_has_no_id_nor_owner() {
    let p = generate(TR);
    let create = p.read("main/src/applications/create_task_use_case.rs");
    let input = &create[create.find("pub struct CreateTaskUseCaseInput").unwrap()..];
    let input = &input[..input.find('}').unwrap()];
    assert!(!input.contains("pub id:") && !input.contains("pub user_id:"), "{input}");
}

#[test]
fn derived_types_compute_their_fields() {
    let p = generate(TR);
    let new_task = p.read("main/src/domains/new_task.rs");
    assert!(new_task.contains("pub title: String,") && !new_task.contains("pub id"), "{new_task}");
    // Not used by an app: generate the types alone (flat crate).
    let types = generate(
        "type Task { id: Int, user_id: Int, title: String, note: Option<String> }\ntype TaskDraft = Partial<Pick<Task, title, note>> + { tags: List<String> }",
    );
    let draft = types.read("src/domains/task_draft.rs");
    assert!(draft.contains("pub title: Option<String>,"), "{draft}");
    assert!(draft.contains("pub note: Option<String>,"), "{draft}");
    assert!(draft.contains("pub tags: Vec<String>,"), "{draft}");

    let err = try_generate(&TR.replace("Omit<Task, id, user_id>", "Omit<Task, nope>")).unwrap_err();
    assert!(err.contains("'Task' has no field 'nope'"), "{err}");
}

#[test]
fn databases_are_shared_and_env_defaults_apply() {
    let p = generate(TR);
    let main = p.read("main/src/main.rs");
    assert!(
        main.contains(r#"trust_connect(&mut db_pools, std::env::var("MY_DB").unwrap_or_else(|_| "postgres://localhost/app".to_string()).to_string(), 4)"#)
            || main.contains(r#"std::env::var("MY_DB").unwrap_or_else(|_| "postgres://localhost/app".to_string())"#),
        "{main}"
    );
    assert!(main.contains(r#"std::env::var("PORT").unwrap_or_else(|_| "8080".to_string())"#), "{main}");
    assert!(p.read("main/Cargo.toml").contains("chrono-tz = \"0.10\""));
}

#[test]
fn unknown_resource_is_rejected() {
    let err = try_generate(&TR.replace("resources { Tasks }", "resources { Nope }")).unwrap_err();
    assert!(err.contains("unknown resource or journal (with undo) 'Nope'"), "{err}");
}
