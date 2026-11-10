#[path = "support/mod.rs"]
mod support;

use support::generate;

const ORDER_TR: &str = r#"type Order {
    id: Int,
    paid: Bool,
    note: Option<String>,
    total: Option<Decimal>,
    created_at: DateTime
}

type PlaceOrderRequest {
    paid: Bool,
    note: Option<String>,
    total: Option<Decimal>
}

infra OrderInfra {
    storage {
        psql {
            entity: Order
        }
    }
}

use_case PlaceOrderUseCase {
    input: PlaceOrderRequest
    output: Order
    call: OrderInfra.save_one
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            POST "/orders" {
                use_case: PlaceOrderUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}
"#;

#[test]
fn bool_datetime_and_option_fields_map_to_rust_types() {
    let project = generate(ORDER_TR);
    let domain = project.read("main/src/domains/order.rs");
    assert!(domain.contains("use chrono::{DateTime, Utc};"), "{domain}");
    assert!(domain.contains("use rust_decimal::Decimal;"), "{domain}");
    assert!(domain.contains("pub paid: bool"), "{domain}");
    assert!(domain.contains("pub note: Option<String>"), "{domain}");
    assert!(domain.contains("pub total: Option<Decimal>"), "{domain}");
    assert!(domain.contains("pub created_at: DateTime<Utc>"), "{domain}");
    assert!(
        !domain.contains("crate::{Option"),
        "Option must never be imported as a domain type:\n{domain}"
    );
}

#[test]
fn datetime_pulls_in_chrono_and_the_sqlx_chrono_feature() {
    let cargo = generate(ORDER_TR).read("main/Cargo.toml");
    assert!(cargo.contains("chrono"), "{cargo}");
    let sqlx_line = cargo
        .lines()
        .find(|l| l.starts_with("sqlx"))
        .expect("sqlx dependency");
    assert!(sqlx_line.contains("\"chrono\""), "{sqlx_line}");
}

#[test]
fn a_datetime_field_missing_from_the_input_is_stamped_with_now() {
    let use_case = generate(ORDER_TR).read("main/src/applications/place_order_use_case.rs");
    assert!(
        use_case.contains("created_at: chrono::Utc::now(),"),
        "{use_case}"
    );
    assert!(use_case.contains("note: input.note,"), "{use_case}");
}
