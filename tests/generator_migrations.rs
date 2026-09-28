#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

fn sticker_tr(seed: Option<&str>) -> String {
    let seed_line = seed.map(|p| format!("seed: \"{}\"", p)).unwrap_or_default();
    format!(
        r#"type Sticker {{
    id: Int,
    title: String,
    price: Decimal,
    published: Bool,
    note: Option<String>
}}

infra StickerInfra {{
    storage {{
        psql {{
            entity: Sticker
            {seed_line}
        }}
    }}
}}

use_case ListStickersUseCase {{
    call: StickerInfra.list
}}

adapter HttpApi {{
    http {{
        on: "0.0.0.0:8080"
        routes {{
            GET "/stickers" {{
                use_case: ListStickersUseCase
            }}
        }}
    }}
}}

app Main: run {{
    HttpApi
}}
"#
    )
}

#[test]
fn every_psql_entity_gets_a_create_table_migration() {
    let project = generate(&sticker_tr(None));
    let sql = project.read("main/migrations/stickers.sql");
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS stickers ("),
        "{sql}"
    );
    assert!(sql.contains("id BIGSERIAL PRIMARY KEY"), "{sql}");
    assert!(sql.contains("price NUMERIC NOT NULL"), "{sql}");
    assert!(sql.contains("published BOOLEAN NOT NULL"), "{sql}");
    assert!(
        sql.contains("ALTER TABLE stickers ADD COLUMN IF NOT EXISTS note TEXT;"),
        "{sql}"
    );
    assert!(!project.exists("main/migrations/stickers.seed.sql"));
}

#[test]
fn main_applies_the_migration_right_after_connecting() {
    let main = generate(&sticker_tr(None)).read("main/src/main.rs");
    let connect = main.find("SQLConnector::connect").expect("connect");
    let migrate = main
        .find(r#"include_str!("../migrations/stickers.sql")"#)
        .unwrap_or_else(|| panic!("migration not applied:\n{main}"));
    assert!(connect < migrate, "{main}");
    assert!(main.contains("sqlx::raw_sql"), "{main}");
}

#[test]
fn a_seed_file_is_copied_and_applied_after_the_table() {
    let dir = tempfile::tempdir().unwrap();
    let seed_path = dir.path().join("stickers.sql");
    std::fs::write(
        &seed_path,
        "INSERT INTO stickers (title) VALUES ('hi') ON CONFLICT DO NOTHING;\n",
    )
    .unwrap();

    let project = generate(&sticker_tr(Some(seed_path.to_str().unwrap())));
    assert!(
        project
            .read("main/migrations/stickers.seed.sql")
            .contains("ON CONFLICT DO NOTHING")
    );

    let main = project.read("main/src/main.rs");
    let table = main
        .find(r#"include_str!("../migrations/stickers.sql")"#)
        .unwrap();
    let seed = main
        .find(r#"include_str!("../migrations/stickers.seed.sql")"#)
        .unwrap();
    assert!(table < seed, "{main}");
}

#[test]
fn a_missing_seed_file_is_a_clear_error() {
    let err = try_generate(&sticker_tr(Some("/nonexistent/seed.sql"))).unwrap_err();
    assert!(
        err.contains("Failed to read seed file '/nonexistent/seed.sql'"),
        "{err}"
    );
}
