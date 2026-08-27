#[path = "support/mod.rs"]
mod support;

use support::generate;

#[test]
fn float_field_maps_to_f64_with_no_extra_dependency() {
    let project = generate(
        r#"type Spot {
            id: Int,
            lat: Float,
            lng: Float
        }"#,
    );

    let domain = project.read("src/domains/spot.rs");
    assert!(domain.contains("pub lat: f64,"));
    assert!(domain.contains("pub lng: f64,"));
    assert!(!domain.contains("rust_decimal"));

    let cargo_toml = project.read("Cargo.toml");
    assert!(!cargo_toml.contains("rust_decimal"));
}

#[test]
fn decimal_field_imports_rust_decimal_and_requires_the_serde_str_dependency() {
    let project = generate(
        r#"type Price {
            id: Int,
            amount: Decimal
        }"#,
    );

    let domain = project.read("src/domains/price.rs");
    assert!(domain.contains("use rust_decimal::Decimal;"));
    assert!(domain.contains("pub amount: Decimal,"));

    let cargo_toml = project.read("Cargo.toml");
    assert!(cargo_toml.contains("rust_decimal = { version = \"1\", features = [\"serde-str\"] }"));
}

#[test]
fn decimal_storage_entity_adds_the_sqlx_rust_decimal_feature() {
    // Regression: a Decimal field on a *storage* entity must also flip on
    // sqlx's own "rust_decimal" feature (same pattern as Uuid's "uuid"
    // feature) — otherwise `sqlx::FromRow`/`query_as` won't compile against
    // a NUMERIC column.
    let project = generate(
        r#"type Price {
            id: Int,
            amount: Decimal
        }
        infra PriceInfra {
            storage {
                psql {
                    entity: Price
                }
            }
        }"#,
    );

    let cargo_toml = project.read("Cargo.toml");
    let sqlx_line = cargo_toml
        .lines()
        .find(|l| l.starts_with("sqlx ="))
        .expect("sqlx dependency line should be present");
    assert!(
        sqlx_line.contains("rust_decimal"),
        "sqlx line missing rust_decimal feature: {sqlx_line}"
    );
}

#[test]
fn float_and_decimal_fields_coexist_on_the_same_struct() {
    let project = generate(
        r#"type Spot {
            id: Int,
            lat: Float,
            lng: Float,
            listing_price: Decimal
        }"#,
    );

    let domain = project.read("src/domains/spot.rs");
    assert!(domain.contains("use rust_decimal::Decimal;"));
    assert!(domain.contains("pub lat: f64,"));
    assert!(domain.contains("pub listing_price: Decimal,"));
}
