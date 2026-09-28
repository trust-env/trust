#[path = "support/mod.rs"]
mod support;

/// Builds (real offline `cargo build`) a project exercising the
/// app-platform features together: login-only `auth: required`, `owner:`
/// scoping on every CRUD verb, and literal `filters {}`.
#[test]
fn owner_scoped_project_compiles() {
    let project = support::generate(
        r#"type CartItem {
            id: Int,
            user_id: Int,
            sticker_id: Int,
            quantity: Int
        }
        type Sticker {
            id: Int,
            title: String,
            visibility: String
        }
        infra CartInfra {
            storage {
                psql {
                    entity: CartItem
                }
            }
        }
        infra StickerInfra {
            storage {
                psql {
                    entity: Sticker
                }
            }
        }
        use_case AddCartItemUseCase {
            call: CartInfra.save_one
            owner: user_id
        }
        use_case ListCartUseCase {
            call: CartInfra.list
            owner: user_id
        }
        use_case GetCartItemUseCase {
            call: CartInfra.find_one
            owner: user_id
        }
        use_case UpdateCartItemUseCase {
            call: CartInfra.update_one
            owner: user_id
        }
        use_case RemoveCartItemUseCase {
            call: CartInfra.delete
            owner: user_id
        }
        use_case ListCommunityStickersUseCase {
            call: StickerInfra.list
            filters {
                visibility: "community"
            }
        }
        adapter HttpApi {
            http {
                on: "0.0.0.0:8080"
                cors: "${CORS_ORIGINS}"
                routes {
                    POST "/cart/items" {
                        use_case: AddCartItemUseCase
                        auth: required
                    }
                    GET "/cart" {
                        use_case: ListCartUseCase
                        auth: required
                    }
                    GET "/cart/items/:id" {
                        use_case: GetCartItemUseCase
                        auth: required
                    }
                    PUT "/cart/items/:id" {
                        use_case: UpdateCartItemUseCase
                        auth: required
                    }
                    DELETE "/cart/items/:id" {
                        use_case: RemoveCartItemUseCase
                        auth: required
                    }
                    GET "/stickers/community" {
                        use_case: ListCommunityStickersUseCase
                    }
                }
            }
        }
        app Main: run {
            HttpApi
        }"#,
    );
    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

/// `Bool`, `DateTime` and `Option<T>` fields round-trip through a psql
/// entity, an explicit input and a path-param split route.
#[test]
fn new_scalar_types_project_compiles() {
    let project = support::generate(
        r#"type Order {
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
        use_case UpdateOrderUseCase {
            call: OrderInfra.update_one
        }
        use_case ListOrdersUseCase {
            call: OrderInfra.list
        }
        adapter HttpApi {
            http {
                on: "0.0.0.0:8080"
                routes {
                    POST "/orders" {
                        use_case: PlaceOrderUseCase
                    }
                    PUT "/orders/:id" {
                        use_case: UpdateOrderUseCase
                    }
                    GET "/orders" {
                        use_case: ListOrdersUseCase
                    }
                }
            }
        }
        app Main: run {
            HttpApi
        }"#,
    );
    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

/// `code {}` use cases with psql + `http_client` + `s3` dependencies, extra
/// crates via `deps {}`, and a `RawRequest` webhook verifying an HMAC —
/// the bodies below use each dependency for real, so this proves the
/// generated scaffolding exposes what a hand-written body needs.
#[test]
fn code_use_cases_with_http_client_and_raw_request_compile() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = dir.path().join("checkout.rs");
    std::fs::write(
        &checkout,
        r#"let lines = self
    .cart_infra
    .list(StorageListQuery {
        limit: 100,
        offset: 0,
        predicates: vec![StoragePredicate {
            field: "user_id".to_string(),
            operator: StoragePredicateOperator::Eq,
            value: user.to_string(),
        }],
    })
    .await?;
if lines.is_empty() {
    return Err(CheckoutUseCaseError::EmptyCart("cart is empty".to_string()));
}
let quantity: i64 = lines.iter().map(|l| l.quantity).sum();
let upload = self.media_infra.presigned_put_url(&format!("u{}/receipt.png", user)).await?;
let purchase: serde_json::Value = self
    .payment_api
    .post_json("/api/v1/products/purchase", &serde_json::json!({ "quantity": quantity, "currency": input.currency, "upload": upload }))
    .await?;
let checkout_url = purchase["checkout_url"].as_str().unwrap_or_default().to_string();
Ok(CheckoutResponse { checkout_url })
"#,
    )
    .unwrap();
    let webhook = dir.path().join("webhook.rs");
    std::fs::write(
        &webhook,
        r#"use hmac::{Hmac, Mac};
let signature = input.headers.get("x-payment-signature").cloned().unwrap_or_default();
let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"secret")
    .map_err(|e| PaymentWebhookUseCaseError::InvalidSignature(e.to_string()))?;
mac.update(input.body.as_bytes());
if hex::encode(mac.finalize().into_bytes()) != signature {
    return Err(PaymentWebhookUseCaseError::InvalidSignature("mismatch".to_string()));
}
Ok(WebhookAck { received: input.body.len() as i64 })
"#,
    )
    .unwrap();

    let project = support::generate(&format!(
        r#"type CartLine {{
            id: Int,
            user_id: Int,
            quantity: Int
        }}
        type Media {{
            key: String
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
        infra MediaInfra {{
            storage {{
                s3 {{
                    bucket: "media"
                    entity: Media
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
                infra: MediaInfra
                infra: PaymentApi
                errors {{ EmptyCart }}
            }}
        }}
        use_case PaymentWebhookUseCase {{
            input: RawRequest
            output: WebhookAck
            code {{
                file: "{webhook}"
                errors {{ InvalidSignature }}
                deps {{ hmac: "0.12" sha2: "0.10" hex: "0.4" }}
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
                        status {{
                            InvalidSignature -> 401
                        }}
                    }}
                }}
            }}
        }}
        app Main: run {{
            HttpApi
        }}"#,
        checkout = checkout.display(),
        webhook = webhook.display(),
    ));
    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}
