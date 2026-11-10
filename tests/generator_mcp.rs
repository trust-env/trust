#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

fn tr(auth: &str) -> String {
    format!(
        r#"config {{ json_case: "camel" }}
enum Domain {{ pro perso }}
type InboxItem {{ id: Int, user_id: Int, title: String, domain: Domain }}
infra InboxInfra {{ storage {{ psql {{ entity: InboxItem }} }} }}
use_case CaptureUseCase {{ call: InboxInfra.save_one owner: user_id }}
use_case ListInboxUseCase {{ call: InboxInfra.list owner: user_id }}
adapter Agent {{
    mcp {{
        on: "127.0.0.1:${{MCP_PORT}}"
        {auth}
        instructions: "Inbox tools."
        tools {{
            capture {{ use_case: CaptureUseCase description: "Add an item" }}
            list_inbox {{ use_case: ListInboxUseCase }}
        }}
    }}
}}
app Main: run {{ Agent }}
"#
    )
}

#[test]
fn tools_get_json_schemas_from_the_use_case_inputs() {
    let p = generate(&tr(r#"auth { secret: "${MCP_TOKEN}" user: 1 }"#));
    let agent = p.read("main/src/adapters/mcp/agent.rs");
    assert!(agent.contains(r#""name": "capture","#), "{agent}");
    assert!(agent.contains(r#""description": "Add an item","#), "{agent}");
    // Undescribed tools fall back to the use case name.
    assert!(agent.contains(r#""description": "ListInboxUseCase","#), "{agent}");
    // camelCase wire names and enum values reach the schema.
    assert!(agent.contains(r#"\"enum\":[\"pro\",\"perso\"]"#), "{agent}");
    assert!(!agent.contains("userId"), "the owner is not an argument:\n{agent}");
    assert!(agent.contains("serde_json::from_value::<CaptureUseCaseInput>(arguments)"), "{agent}");
    assert!(agent.contains(r#".route("/mcp", post(mcp_post::<C0, C1>))"#), "{agent}");
    assert!(agent.contains(r#"Ok(1)"#), "{agent}");
    assert!(p.read("main/src/adapters.rs").contains("mod mcp;"));
}

#[test]
fn main_serves_mcp_over_http_and_stdio() {
    let p = generate(&tr("auth: required"));
    let main = p.read("main/src/main.rs");
    assert!(main.contains(r#"Some("mcp")"#), "{main}");
    assert!(main.contains("server.serve_stdio(user)"), "{main}");
    assert!(main.contains("start_http_server(agent.router()"), "{main}");
    assert!(main.contains(".with_writer(std::io::stderr)"), "{main}");
    assert!(p.read("main/src/adapters/mcp/agent.rs").contains("jsonwebtoken::decode::<Claims>"));
}

#[test]
fn owner_tools_need_auth() {
    let err = try_generate(&tr("")).unwrap_err();
    assert!(err.contains("must declare 'auth'"), "{err}");
}

#[test]
fn unknown_tool_use_case_is_rejected() {
    let err = try_generate(&tr("auth: required").replace("use_case: ListInboxUseCase", "use_case: NopeUseCase"))
        .unwrap_err();
    assert!(err.contains("unknown use_case 'NopeUseCase'") || err.contains("NopeUseCase"), "{err}");
}
