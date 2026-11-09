#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

fn tr(schedule: &str) -> String {
    format!(
        r#"type Tick {{ id: Int, label: String }}
infra TickInfra {{ storage {{ psql {{ entity: Tick }} }} }}
use_case CreateTickUseCase {{ call: TickInfra.save_one }}
use_case ListTicksUseCase {{
    call: TickInfra.list
    filters {{ label: input.label }}
}}
adapter Api {{ http {{ on: "0.0.0.0:8080" routes {{ GET "/ticks" {{ use_case: ListTicksUseCase }} }} }} }}
{schedule}
app Main: run {{ Api Jobs }}
"#
    )
}

#[test]
fn schedule_jobs_are_spawned_with_compiled_cron_masks() {
    let p = generate(&tr(
        r#"schedule Jobs {
            tz: "Europe/Paris"
            user: 3
            "*/15 * * * *" => CreateTickUseCase
            "30 7 * * mon-fri" => ListTicksUseCase tz: "UTC"
        }"#,
    ));
    let main = p.read("main/src/main.rs");
    assert!(main.contains("use common::UseCase;"), "{main}");
    assert!(main.contains("// schedule: \"*/15 * * * *\" in \"Europe/Paris\""), "{main}");
    assert!(main.contains("minutes: 0x200040008001,"), "{main}");
    assert!(main.contains("weekdays: 0x3e,"), "{main}");
    assert!(main.contains("run_scheduled_job(\"Jobs.CreateTickUseCase\", cron, tz"), "{main}");
    assert!(main.contains(".execute(3, Default::default())"), "{main}");
    assert!(main.contains("schedule_job_1_task.await??;"), "{main}");

    let runtime = p.read("main/src/trust_schedule.rs");
    assert!(runtime.contains("pub struct TrustCron"), "{runtime}");
    assert!(p.read("main/src/lib.rs").contains("mod trust_schedule;"));

    let cargo = p.read("main/Cargo.toml");
    assert!(cargo.contains("chrono-tz = \"0.10\""), "{cargo}");
    assert!(cargo.contains("tracing = \"0.1\""), "{cargo}");

    // Scheduled runs build their input with Default::default().
    let uc = p.read("main/src/applications/list_ticks_use_case.rs");
    assert!(uc.contains("#[derive(Debug, Clone, Default, serde::Deserialize)]"), "{uc}");
}

#[test]
fn bad_cron_time_zone_and_use_case_are_compile_errors() {
    let cron = try_generate(&tr(r#"schedule Jobs { "61 * * * *" => CreateTickUseCase }"#)).unwrap_err();
    assert!(cron.contains("out of range for the minute field"), "{cron}");

    let tz = try_generate(&tr(r#"schedule Jobs { tz: "Mars/Olympus" "* * * * *" => CreateTickUseCase }"#))
        .unwrap_err();
    assert!(tz.contains("unknown time zone 'Mars/Olympus'"), "{tz}");

    let use_case = try_generate(&tr(r#"schedule Jobs { "* * * * *" => NopeUseCase }"#)).unwrap_err();
    assert!(use_case.contains("unknown use_case 'NopeUseCase'"), "{use_case}");
}

#[test]
fn today_in_a_time_zone_filters_by_the_local_date() {
    let p = generate(&tr(r#"schedule Jobs { "0 * * * *" => CreateTickUseCase }"#).replace(
        "filters { label: input.label }",
        r#"filters { label >= today("Europe/Paris") }"#,
    ));
    let uc = p.read("main/src/applications/list_ticks_use_case.rs");
    assert!(
        uc.contains(r#"chrono::Utc::now().with_timezone(&"Europe/Paris".parse::<chrono_tz::Tz>()"#),
        "{uc}"
    );
}
