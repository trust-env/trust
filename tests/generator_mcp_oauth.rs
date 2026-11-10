#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const TR: &str = r#"type Goal { id: Int, user_id: Int, title: String }
infra GoalInfra { storage { psql { entity: Goal owner: user_id } } }
use_case ListGoalsUseCase { call: GoalInfra.list }
use_case CreateGoalUseCase { call: GoalInfra.save_one }
adapter Agent {
    mcp {
        on: "127.0.0.1:8090"
        oauth { public_url: "${MCP_PUBLIC_URL}" password: "${MCP_OAUTH_PASSWORD}" user: 1 }
        tools { create_goal { use_case: CreateGoalUseCase } }
        resources { goals { use_case: ListGoalsUseCase uri: "world://goals" description: "Every goal" } }
    }
}
app Main: run { Agent }
"#;

#[test]
fn resources_are_listed_and_read_through_their_use_case() {
    let agent = generate(TR).read("main/src/adapters/mcp/agent.rs");
    assert!(agent.contains(r#""resources": { "listChanged": false }"#), "{agent}");
    assert!(agent.contains(r#"{ "uri": "world://goals", "name": "goals", "description": "Every goal", "mimeType": "application/json" },"#), "{agent}");
    assert!(agent.contains(r#""world://goals" => match self.list_goals_use_case.execute(user, ListGoalsUseCaseInput::default()).await {"#), "{agent}");
}

#[test]
fn tool_errors_read_as_text() {
    let agent = generate(TR).read("main/src/adapters/mcp/agent.rs");
    assert!(agent.contains("Err(error) => Err(describe_error(format!(\"{:?}\", error))),"), "{agent}");
    assert!(agent.contains("fn describe_error(debug: String) -> String {"), "{agent}");
}

#[test]
fn oauth_serves_metadata_registration_authorization_and_tokens() {
    let p = generate(TR);
    let agent = p.read("main/src/adapters/mcp/agent.rs");
    for route in [
        r#".route("/.well-known/oauth-protected-resource", axum::routing::get(oauth_protected_resource))"#,
        r#".route("/.well-known/oauth-authorization-server", axum::routing::get(oauth_authorization_server))"#,
        r#".route("/oauth/register", post(oauth_register))"#,
        r#".route("/oauth/authorize", axum::routing::get(oauth_authorize_page).post(oauth_authorize))"#,
        r#".route("/oauth/token", post(oauth_token))"#,
    ] {
        assert!(agent.contains(route), "missing {route}\n{agent}");
    }
    assert!(agent.contains(r#"Bearer resource_metadata="{}""#), "{agent}");
    assert!(agent.contains("PKCE verification failed"), "{agent}");
    assert!(agent.contains(r#"format!("{}:refresh", secret)"#), "{agent}");
    let cargo = p.read("main/Cargo.toml");
    for dep in ["sha2", "base64", "jsonwebtoken", "\"v4\""] {
        assert!(cargo.contains(dep), "missing {dep} in {cargo}");
    }
}

#[test]
fn resources_need_a_known_use_case() {
    let err = try_generate(&TR.replace("use_case: ListGoalsUseCase uri", "use_case: NopeUseCase uri")).unwrap_err();
    assert!(err.contains("unknown use_case 'NopeUseCase'"), "{err}");
}
