#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

fn tr(adapter_options: &str, extra: &str) -> String {
    format!(
        r#"{extra}
type Task {{ id: Int, user_id: Int, task_title: String }}
infra TaskInfra {{ storage {{ psql {{ entity: Task }} }} }}
infra FeedApi {{ http_client {{ base_url: "${{FEED_URL}}" retry: 3 timeout: 5 }} }}
use_case CreateTaskUseCase {{ call: TaskInfra.save_one owner: user_id }}
use_case ListTasksUseCase {{ call: TaskInfra.list owner: user_id }}
use_case InternalCreateUseCase {{ call: TaskInfra.save_one owner: user_id }}
adapter Api {{
    http {{
        on: "0.0.0.0:8080"
        {adapter_options}
        routes {{
            POST "/tasks" {{ use_case: CreateTaskUseCase auth: required }}
            GET "/tasks" {{ use_case: ListTasksUseCase auth: required }}
            POST "/internal/tasks" {{
                use_case: InternalCreateUseCase
                auth {{ secret: "${{INTERNAL_TOKEN}}" header: "X-Internal-Token" user: 1 }}
            }}
        }}
    }}
}}
app Main: run {{ Api }}
"#
    )
}

fn handler(p: &support::GeneratedProject, name: &str) -> String {
    p.read(&format!("main/src/adapters/http/api/{}.rs", name))
}

#[test]
fn every_http_adapter_gets_health_request_logs_and_the_token_command() {
    let p = generate(&tr("", ""));
    let adapter = p.read("main/src/adapters/http/api.rs");
    assert!(adapter.contains(r#".route("/health", get(health))"#), "{adapter}");
    assert!(adapter.contains(".layer(tower_http::trace::TraceLayer::new_for_http())"), "{adapter}");

    let main = p.read("main/src/main.rs");
    assert!(main.contains("init_logging();"), "{main}");
    assert!(main.contains(r#"Some("token")"#), "{main}");
    assert!(main.contains("jsonwebtoken::encode("), "{main}");

    let cargo = p.read("main/Cargo.toml");
    assert!(cargo.contains("tracing-subscriber"), "{cargo}");
    assert!(cargo.contains("\"trace\""), "{cargo}");
    assert!(cargo.contains("jsonwebtoken = \"9\""), "{cargo}");
}

#[test]
fn secret_auth_checks_a_header_and_acts_as_its_user() {
    let p = generate(&tr("", ""));
    let h = handler(&p, "internal_create_use_case_post");
    assert!(h.contains("auth_headers: axum::http::HeaderMap,"), "{h}");
    assert!(h.contains(r#".get("x-internal-token")"#), "{h}");
    assert!(h.contains(r#"std::env::var("INTERNAL_TOKEN")"#), "{h}");
    assert!(h.contains(".execute(1, input)"), "{h}");
    assert!(!h.contains("UserContext"), "{h}");
}

#[test]
fn envelope_false_returns_the_entity_itself() {
    let p = generate(&tr("envelope: false", ""));
    let create = handler(&p, "create_task_use_case_post");
    assert!(create.contains("-> HttpResult<Task> {"), "{create}");
    assert!(create.contains("Ok(Json(response.task))"), "{create}");
    let list = handler(&p, "list_tasks_use_case_get");
    assert!(list.contains("-> HttpResult<Vec<Task>> {"), "{list}");
    assert!(list.contains("Ok(Json(response.tasks))"), "{list}");
}

#[test]
fn camel_case_config_renames_every_json_field() {
    let p = generate(&tr("", r#"config { json_case: "camel" }"#));
    let domain = p.read("main/src/domains/task.rs");
    assert!(domain.contains("#[serde(rename_all = \"camelCase\")]\npub struct Task"), "{domain}");
    let uc = p.read("main/src/applications/create_task_use_case.rs");
    assert!(uc.contains("#[serde(rename_all = \"camelCase\")]\npub struct CreateTaskUseCaseInput"), "{uc}");
    assert!(uc.contains("#[serde(rename_all = \"camelCase\")]\npub struct CreateTaskUseCaseOutput"), "{uc}");
}

#[test]
fn events_stream_relays_notifying_use_cases() {
    let p = generate(&tr(r#"events: "/events" auth: required"#, ""));
    let adapter = p.read("main/src/adapters/http/api.rs");
    assert!(adapter.contains(r#".route("/events", get(events_stream))"#), "{adapter}");
    assert!(adapter.contains(r#"query.get("access_token")"#), "{adapter}");
    assert!(p.read("main/src/trust_events.rs").contains("pub fn trust_publish"));

    // Writes notify by default, reads don't.
    let create = p.read("main/src/applications/create_task_use_case.rs");
    assert!(create.contains(r#"crate::trust_publish("CreateTaskUseCase", user);"#), "{create}");
    let list = p.read("main/src/applications/list_tasks_use_case.rs");
    assert!(!list.contains("trust_publish"), "{list}");

    let cargo = p.read("main/Cargo.toml");
    assert!(cargo.contains("tokio-stream"), "{cargo}");
}

#[test]
fn notify_false_silences_a_write() {
    let p = generate(
        &tr(r#"events: "/events""#, "")
            .replace("use_case CreateTaskUseCase { call: TaskInfra.save_one owner: user_id }", "use_case CreateTaskUseCase { call: TaskInfra.save_one owner: user_id notify: false }"),
    );
    let create = p.read("main/src/applications/create_task_use_case.rs");
    assert!(!create.contains("trust_publish"), "{create}");
}

#[test]
fn http_client_retries_and_reads_text() {
    let src = tr("", "").replace(
        "use_case ListTasksUseCase",
        "type F { x: Int }\nuse_case FeedUseCase { input: F output: F code { file: \"FILE\" infra: FeedApi } }\nuse_case ListTasksUseCase",
    );
    let dir = tempfile::tempdir().unwrap();
    let body = dir.path().join("feed.rs");
    std::fs::write(&body, "Ok(input)\n").unwrap();
    let src = src.replace("FILE", &body.display().to_string()).replace(
        r#"POST "/tasks" { use_case: CreateTaskUseCase auth: required }"#,
        r#"POST "/tasks" { use_case: CreateTaskUseCase auth: required } POST "/feed" { use_case: FeedUseCase }"#,
    );
    let p = generate(&src);
    let client = p.read("main/src/infrastructures/http_client/feed_api.rs");
    assert!(client.contains("const RETRIES: u32 = 3;"), "{client}");
    assert!(client.contains(".timeout(std::time::Duration::from_secs(5))"), "{client}");
    assert!(client.contains("pub async fn get_text("), "{client}");
    assert!(client.contains("pub async fn post_form<"), "{client}");
    assert!(client.contains("pub fn request(&self, method: reqwest::Method"), "{client}");
}

#[test]
fn secret_and_infra_together_are_rejected() {
    let err = try_generate(&tr("", "").replace(
        r#"auth { secret: "${INTERNAL_TOKEN}" header: "X-Internal-Token" user: 1 }"#,
        r#"auth { secret: "${INTERNAL_TOKEN}" infra: TaskInfra }"#,
    ))
    .unwrap_err();
    assert!(err.contains("either 'secret' or 'infra'"), "{err}");
}
