#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

/// Writes `body` to a temp file and returns the `.tr` source for a shop
/// exposing a `code {}` checkout (psql + http_client deps) and a
/// `RawRequest` webhook.
fn shop_tr(dir: &std::path::Path, checkout_body: &str) -> String {
    let checkout = dir.join("checkout.rs");
    let webhook = dir.join("webhook.rs");
    std::fs::write(&checkout, checkout_body).unwrap();
    std::fs::write(&webhook, "Ok(WebhookAck { received: input.body.len() as i64 })\n").unwrap();
    format!(
        r#"type CartLine {{
    id: Int,
    user_id: Int,
    quantity: Int
}}

type CheckoutRequest {{
    currency: String
}}

type CheckoutResponse {{
    checkout_url: String
}}

type WebhookAck {{
    received: Int
}}

infra CartInfra {{
    storage {{
        psql {{
            entity: CartLine
        }}
    }}
}}

infra PaymentApi {{
    http_client {{
        base_url: "${{PAYMENT_API_URL}}"
        headers {{
            "X-API-Key": "${{PAYMENT_API_KEY}}"
        }}
    }}
}}

use_case CheckoutUseCase {{
    input: CheckoutRequest
    output: CheckoutResponse
    code {{
        file: "{checkout}"
        infra: CartInfra
        infra: PaymentApi
        errors {{ EmptyCart }}
        deps {{ hex: "0.4" }}
    }}
}}

use_case PaymentWebhookUseCase {{
    input: RawRequest
    output: WebhookAck
    code {{
        file: "{webhook}"
    }}
}}

adapter HttpApi {{
    http {{
        on: "0.0.0.0:8080"
        routes {{
            POST "/checkout" {{
                use_case: CheckoutUseCase
                auth: required
                status {{
                    EmptyCart -> 409
                }}
            }}
            POST "/webhooks/payment" {{
                use_case: PaymentWebhookUseCase
            }}
        }}
    }}
}}

app Main: run {{
    HttpApi
}}
"#,
        checkout = checkout.display(),
        webhook = webhook.display(),
    )
}

const CHECKOUT_BODY: &str = "Err(CheckoutUseCaseError::EmptyCart(format!(\"user {} has no cart\", user)))\n";

#[test]
fn code_body_is_spliced_into_execute_with_the_user() {
    let dir = tempfile::tempdir().unwrap();
    let use_case = generate(&shop_tr(dir.path(), CHECKOUT_BODY)).read("main/src/applications/checkout_use_case.rs");
    assert!(use_case.contains("async fn execute(&self, user: i64, input: CheckoutRequest)"), "{use_case}");
    assert!(use_case.contains("        Err(CheckoutUseCaseError::EmptyCart(format!("), "{use_case}");
    assert!(use_case.contains("use crate::*;"), "{use_case}");
}

#[test]
fn code_infras_become_fields_generic_for_storage_concrete_for_http_clients() {
    let dir = tempfile::tempdir().unwrap();
    let use_case = generate(&shop_tr(dir.path(), CHECKOUT_BODY)).read("main/src/applications/checkout_use_case.rs");
    assert!(use_case.contains("pub struct CheckoutUseCase<S0>"), "{use_case}");
    // psql storages can also open transactions from the body.
    assert!(use_case.contains("S0: CartLineStorage + TrustTransactional,"), "{use_case}");
    assert!(use_case.contains("pub cart_infra: S0,"), "{use_case}");
    assert!(use_case.contains("pub payment_api: PaymentApiClient,"), "{use_case}");
    assert!(use_case.contains("pub fn new(cart_infra: S0, payment_api: PaymentApiClient)"), "{use_case}");
}

#[test]
fn declared_errors_become_string_variants_mappable_by_routes() {
    let dir = tempfile::tempdir().unwrap();
    let project = generate(&shop_tr(dir.path(), CHECKOUT_BODY));
    let use_case = project.read("main/src/applications/checkout_use_case.rs");
    assert!(use_case.contains("    EmptyCart(String),"), "{use_case}");
    assert!(use_case.contains("impl From<PsqlCartLineStorageError> for CheckoutUseCaseError"), "{use_case}");
    let route = project.read("main/src/adapters/http/http_api/checkout_use_case_post.rs");
    assert!(route.contains("409"), "{route}");
}

#[test]
fn deps_and_http_client_crates_are_added_to_cargo_toml() {
    let dir = tempfile::tempdir().unwrap();
    let cargo = generate(&shop_tr(dir.path(), CHECKOUT_BODY)).read("main/Cargo.toml");
    assert!(cargo.contains("hex = \"0.4\""), "{cargo}");
    let reqwest = cargo.lines().find(|l| l.starts_with("reqwest")).expect("reqwest");
    assert!(reqwest.contains("json"), "{reqwest}");
}

#[test]
fn http_client_infra_is_generated_and_built_from_env_in_main() {
    let dir = tempfile::tempdir().unwrap();
    let project = generate(&shop_tr(dir.path(), CHECKOUT_BODY));
    let client = project.read("main/src/infrastructures/http_client/payment_api.rs");
    assert!(client.contains("pub struct PaymentApiClient"), "{client}");
    assert!(client.contains("pub async fn post_json"), "{client}");
    let main = project.read("main/src/main.rs");
    assert!(main.contains(r#"PaymentApiClient::new(format!("{}", std::env::var("PAYMENT_API_URL")"#), "{main}");
    assert!(main.contains(r#"("X-API-Key".to_string(), format!("{}", std::env::var("PAYMENT_API_KEY")"#), "{main}");
    assert!(main.contains("CheckoutUseCase::new(cart_infra_storage.clone(), payment_api_storage.clone())"), "{main}");
}

#[test]
fn raw_request_routes_hand_over_the_untouched_body_and_headers() {
    let dir = tempfile::tempdir().unwrap();
    let project = generate(&shop_tr(dir.path(), CHECKOUT_BODY));
    assert!(project.read("main/src/domains/raw_request.rs").contains("pub struct RawRequest"));
    let route = project.read("main/src/adapters/http/http_api/payment_webhook_use_case_post.rs");
    assert!(route.contains("headers: axum::http::HeaderMap,\n        body: String,"), "{route}");
    assert!(route.contains("let input = RawRequest {"), "{route}");
    assert!(!route.contains("Json(input)"), "{route}");
}

#[test]
fn a_missing_code_file_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = shop_tr(dir.path(), CHECKOUT_BODY);
    std::fs::remove_file(dir.path().join("checkout.rs")).unwrap();
    let err = try_generate(&src).unwrap_err();
    assert!(err.contains("failed to read code file"), "{err}");
}

#[test]
fn reserved_error_names_cannot_be_redeclared() {
    let dir = tempfile::tempdir().unwrap();
    let src = shop_tr(dir.path(), CHECKOUT_BODY).replace("errors { EmptyCart }", "errors { NotFound }");
    let err = try_generate(&src).unwrap_err();
    assert!(err.contains("'NotFound' is always part of the error enum"), "{err}");
}
