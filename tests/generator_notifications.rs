#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const TR: &str = r#"enum NotificationKind { brief review conflict }
enum NotificationStatus { sent failed logged }
type Notification { id: Int, user_id: Int, kind: NotificationKind, at: DateTime, channel: String, text: String, status: NotificationStatus, error: Option<String>, external_id: Option<String> }
type TextInput { text: String }
type Summary { summary: String }
type Note { id: Int, user_id: Int, text: String }
infra NotificationInfra { storage { psql { entity: Notification owner: user_id } } }
infra NoteInfra { storage { psql { entity: Note owner: user_id } } }
channel Telegram { telegram { token: "${TELEGRAM_BOT_TOKEN}" chat: "${TELEGRAM_CHAT_ID}" } }
channel Team { webhook { url: "${TEAM_HOOK}" format: discord } }
notifications Notify { infra: NotificationInfra default: Telegram fallback: log routing { conflict: [Team, Telegram] } }
use_case JournalUseCase { input: TextInput output: Summary call: NoteInfra.list }
use_case CaptureUseCase { input: TextInput output: Summary call: NoteInfra.list }
bot TelegramBot {
    channel: Telegram  path: "/webhooks/telegram"  secret: "${TELEGRAM_WEBHOOK_SECRET}"  user: 1
    commands { journal: JournalUseCase }
    on_reply { review: JournalUseCase }
    default: CaptureUseCase
    reply: "OK: {result.summary}"
}
adapter Api { http { on: "0.0.0.0:8080" auth: required resources { TelegramBot } routes { POST "/send" { use_case: SendUseCase } } } }
app Main: run { Api }
"#;

fn tr() -> String {
    // Code use cases, so the bot's targets take `{ text }` and return a summary.
    TR.replace(
        "use_case JournalUseCase { input: TextInput output: Summary call: NoteInfra.list }\nuse_case CaptureUseCase { input: TextInput output: Summary call: NoteInfra.list }",
        "use_case JournalUseCase { input: TextInput output: Summary code { file: \"@BODY@\" infra: NoteInfra } }\nuse_case CaptureUseCase { input: TextInput output: Summary code { file: \"@BODY@\" infra: NoteInfra } }\nuse_case SendUseCase { input: TextInput output: Summary code { file: \"@BODY@\" infra: [Notify, NotificationInfra] } }",
    )
}

fn project() -> (tempfile::TempDir, support::GeneratedProject) {
    let dir = tempfile::tempdir().unwrap();
    let body = dir.path().join("body.rs");
    std::fs::write(&body, "let _ = user;\nOk(Summary { summary: input.text })\n").unwrap();
    let p = generate(&tr().replace("@BODY@", &body.display().to_string()));
    (dir, p)
}

#[test]
fn notifications_route_by_kind_with_a_fallback_and_record() {
    let (_dir, p) = project();
    let notify = p.read("main/src/infrastructures/http_client/notify.rs");
    assert!(notify.contains(r#""conflict" => &["Team", "Telegram"],"#), "{notify}");
    assert!(notify.contains(r#"_ => &["Telegram"],"#), "{notify}");
    assert!(notify.contains(r#"if !tried.contains(&"log") {"#), "{notify}");
    assert!(notify.contains("crate::trust_send_telegram(&self.client"), "{notify}");
    assert!(notify.contains(r#"crate::trust_send_webhook(&self.client, &format!("{}", std::env::var("TEAM_HOOK")"#), "{notify}");
    assert!(notify.contains("pub async fn send_recorded("), "{notify}");
    assert!(notify.contains("storage: &impl crate::NotificationStorage,"), "{notify}");
    let shared = p.read("main/src/trust_notify.rs");
    assert!(shared.contains(r#"replace(token.trim(), "<token>")"#), "{shared}");
}

#[test]
fn bots_verify_their_secret_and_dispatch_commands() {
    let (_dir, p) = project();
    let bot = p.read("main/src/applications/telegram_bot_use_case.rs");
    assert!(bot.contains(r#"input.headers.get("x-telegram-bot-api-secret-token")"#), "{bot}");
    assert!(bot.contains(r#"(_, Some("journal")) => Some("#), "{bot}");
    assert!(bot.contains(r#"(Some("review"), _) => Some("#), "{bot}");
    assert!(bot.contains(r#".eq("user_id", user).eq("kind", kind).eq("external_id", id)"#), "{bot}");
    assert!(bot.contains(r#"reply.push_str("OK: ");"#), "{bot}");
    assert!(bot.contains(r#"self.telegram_bot_channel.send_to("Telegram", "reply", reply, message_id.as_deref())"#), "{bot}");
    let adapter = p.read("main/src/adapters/http/api.rs");
    assert!(adapter.contains(r#".route("/webhooks/telegram", post("#), "{adapter}");
}

#[test]
fn notification_mistakes_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let body = dir.path().join("body.rs");
    std::fs::write(&body, "let _ = user;\nOk(Summary { summary: input.text })\n").unwrap();
    let base = tr().replace("@BODY@", &body.display().to_string());
    let err = try_generate(&base.replace("conflict: [Team, Telegram]", "conflict: [Sms]")).unwrap_err();
    assert!(err.contains("unknown channel 'Sms'"), "{err}");
    let err = try_generate(&base.replace("default: CaptureUseCase", "default: NopeUseCase")).unwrap_err();
    assert!(err.contains("unknown use_case 'NopeUseCase'"), "{err}");
    let err = try_generate(&base.replace("channel: Telegram  path", "channel: Team  path")).unwrap_err();
    assert!(err.contains("must be a telegram channel"), "{err}");
    let err = try_generate(&base.replace("external_id: Option<String> }", "external_id: Option<String>, extra: Int }")).unwrap_err();
    assert!(err.contains("field 'extra' must be optional"), "{err}");
}
