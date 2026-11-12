#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const TR: &str = r#"config { tz: "Europe/Paris" }
enum EventKind { google outlook }
type CalendarEvent { id: Int, user_id: Int, kind: EventKind, external_id: String, title: String, start_at: DateTime, end_at: DateTime, note: Option<String> }
type GoogleItem { id: String, summary: String, start: DateTime, end: DateTime }
type GoogleWindow { from: DateTime, to: DateTime }
type GoogleEvents { items: List<GoogleItem> }
infra EventInfra { storage { psql { entity: CalendarEvent owner: user_id } } }
infra OutlookIcs { ics_feed { url: "${OUTLOOK_ICS_URL}" timezone_fallback: "Europe/Paris" all_day: skip } }
infra GoogleCalendar { http_client {
    base_url: "https://www.googleapis.com/calendar/v3"
    oauth2 { token_url: "https://oauth2.googleapis.com/token" client_id: "${GOOGLE_CLIENT_ID}" client_secret: "${GOOGLE_CLIENT_SECRET}" refresh_token: "${GOOGLE_REFRESH_TOKEN}" }
} }
use_case GoogleEventsUseCase { input: GoogleWindow output: GoogleEvents code { file: "@GOOGLE_RS@" infra: GoogleCalendar } }
mirror OutlookMirror {
    source: OutlookIcs
    into: EventInfra
    key: [user_id, kind, external_id]
    scope { kind: "outlook"  end_at > window.from  start_at < window.to }
    map { kind: "outlook"  external_id: item.instance_id  title: item.title  start_at: item.start  end_at: item.end  note: item.location }
    window { from: week_start("Europe/Paris")  days: 28 }
}
mirror GoogleMirror {
    source: GoogleEventsUseCase
    into: EventInfra
    key: [user_id, kind, external_id]
    scope { kind: "google" }
    map { kind: "google" external_id: item.id title: item.summary start_at: item.start end_at: item.end }
    window { from: today days: 14 }
}
adapter Api { http { on: "0.0.0.0:8080" auth: required routes {
    POST "/sync/outlook" { use_case: OutlookMirrorUseCase }
    POST "/sync/google" { use_case: GoogleMirrorUseCase }
} } }
app Main: run { Api }
"#;

fn tr(dir: &std::path::Path) -> String {
    let google = dir.join("google.rs");
    std::fs::write(&google, "let _ = input;\nlet items = self.google_calendar.get_json_paged(\"/events\", \"items\", \"nextPageToken\").await.map_err(GoogleEventsUseCaseError::StorageError)?;\nOk(GoogleEvents { items })\n").unwrap();
    TR.replace("@GOOGLE_RS@", &google.display().to_string())
}

#[test]
fn http_clients_refresh_oauth2_tokens_and_follow_pages() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let client = p.read("main/src/infrastructures/http_client/google_calendar.rs");
    assert!(client.contains("pub fn with_oauth2("), "{client}");
    assert!(client.contains("this_try = this_try.bearer_auth(token);"), "{client}");
    assert!(client.contains("status == reqwest::StatusCode::UNAUTHORIZED"), "{client}");
    assert!(client.contains("pub async fn get_json_paged<T: DeserializeOwned>("), "{client}");
    let main = p.read("main/src/main.rs");
    assert!(main.contains(".with_oauth2("), "{main}");
    assert!(main.contains("GOOGLE_REFRESH_TOKEN"), "{main}");
}

#[test]
fn ics_feeds_expand_occurrences() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let client = p.read("main/src/infrastructures/http_client/outlook_ics.rs");
    assert!(client.contains(r#"crate::trust_ics_occurrences(&text, from, to, "Europe/Paris", false)"#), "{client}");
    assert!(client.contains("pub fn is_configured(&self) -> bool"), "{client}");
    let ics = p.read("main/src/trust_ics.rs");
    assert!(ics.contains("pub struct IcsOccurrence {"), "{ics}");
    assert!(ics.contains(r#""Romance Standard Time" => "Europe/Paris","#), "{ics}");
    assert!(p.read("main/Cargo.toml").contains("rrule"));
}

#[test]
fn mirrors_upsert_on_their_key_and_remove_what_the_source_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let p = generate(&tr(dir.path()));
    let outlook = p.read("main/src/applications/outlook_mirror_use_case.rs");
    assert!(outlook.contains("self.outlook_ics.occurrences(from, to).await?"), "{outlook}");
    assert!(outlook.contains(r#"external_id: TrustMirrorInto::trust_into(item.instance_id.clone()),"#), "{outlook}");
    assert!(outlook.contains(r#"kind: serde_json::from_str("\"outlook\"")"#), "{outlook}");
    assert!(outlook.contains(r#".eq("user_id", user).eq("kind", "outlook").gt("end_at", from.to_rfc3339()).lt("start_at", to.to_rfc3339())"#), "{outlook}");
    assert!(outlook.contains("today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64)"), "{outlook}");
    let google = p.read("main/src/applications/google_mirror_use_case.rs");
    assert!(google.contains("self.google_events_use_case.execute(user, source_input)"), "{google}");
    assert!(p.read("main/src/domains/mirror_report.rs").contains("pub removed: i64,"));
    let migration = p.read("main/migrations/calendar_events.sql");
    assert!(migration.contains("(user_id, kind, external_id)"), "{migration}");
}

#[test]
fn mirror_mistakes_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let base = tr(dir.path());
    let err = try_generate(&base.replace("map { kind: \"outlook\"  external_id: item.instance_id", "map { kind: \"outlook\"  nope: item.instance_id")).unwrap_err();
    assert!(err.contains("'CalendarEvent' has no field 'nope'"), "{err}");
    let err = try_generate(&base.replace("map { kind: \"google\" external_id: item.id", "map { kind: \"google\"")).unwrap_err();
    assert!(err.contains("key field 'external_id' must be set by map {}"), "{err}");
    let err = try_generate(&base.replace("source: OutlookIcs", "source: GoogleCalendar")).unwrap_err();
    assert!(err.contains("must be an ics_feed infra or a use case"), "{err}");
    let err = try_generate(&base.replace("timezone_fallback: \"Europe/Paris\"", "timezone_fallback: \"Nowhere\"")).unwrap_err();
    assert!(err.contains("unknown timezone_fallback 'Nowhere'"), "{err}");
}

#[test]
fn every_windows_zone_maps_to_a_known_iana_zone() {
    let template = include_str!("../templates/file/http_client/ics.rs.tera");
    let mut count = 0;
    for line in template.lines() {
        let Some((windows, iana)) = line.trim().strip_prefix('"').and_then(|l| l.strip_suffix("\",")).and_then(|l| l.split_once("\" => \"")) else {
            continue;
        };
        assert!(iana.parse::<chrono_tz::Tz>().is_ok(), "{windows} → {iana} is not a chrono-tz zone");
        count += 1;
    }
    assert!(count > 130, "only {count} Windows zones");
    assert!(template.contains(r#""Azerbaijan Standard Time" => "Asia/Baku","#));
    assert!(template.contains("unknown time zone"));
}
