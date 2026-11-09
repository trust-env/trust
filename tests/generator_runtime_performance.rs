#[path = "support/mod.rs"]
mod support;

use support::generate;

/// Covers the generated code's runtime-performance contract: index-friendly
/// SQL, one round-trip per batch, a configurable connection pool, an
/// optimised release profile and a streaming load balancer.
const RATINGS_TR: &str = r#"type Rating {
    id: Int,
    spot_id: Int,
    comment: Option<String>
}

type Permission {
    id: Int,
    user_id: Int,
    target: String,
    action: String
}

infra RatingInfra {
    storage {
        psql {
            entity: Rating
        }
    }
}

infra PermissionInfra {
    storage {
        psql {
            entity: Permission
        }
    }
}

use_case ListRatingsForSpotUseCase {
    call: RatingInfra.list
    filters {
        spot_id: input.spot_id
    }
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
        routes {
            GET "/spots/:spot_id/ratings" {
                use_case: ListRatingsForSpotUseCase
                auth {
                    infra: PermissionInfra
                    target: "rating"
                    action: "list"
                }
            }
        }
    }
}

app Main: run {
    HttpApi
}
"#;

fn rating_storage() -> String {
    generate(RATINGS_TR).read("main/src/infrastructures/storage/psql/rating.rs")
}

#[test]
fn list_casts_the_parameter_never_the_column() {
    let storage = rating_storage();
    assert!(!storage.contains("::text {} ${}"), "column cast is back:\n{storage}");
    assert!(storage.contains(r#""{} {} ${}::{}","#), "{storage}");
    assert!(storage.contains(r#""spot_id" => "BIGINT","#), "{storage}");
    assert!(storage.contains(r#""comment" => "TEXT","#), "{storage}");
}

#[test]
fn list_rejects_unknown_columns_instead_of_splicing_them_into_sql() {
    let storage = rating_storage();
    assert!(
        storage.contains("PsqlRatingStorageError::ConversionError(format!(\n                        \"unknown column '{}' for table ratings\""),
        "{storage}"
    );
}

#[test]
fn list_pages_are_ordered_and_bounded() {
    let storage = rating_storage();
    assert!(storage.contains(r#"sql.push_str(" ORDER BY id");"#), "{storage}");
    assert!(storage.contains("query.limit.clamp(0, MAX_LIST_LIMIT)"), "{storage}");
    assert!(storage.contains("query.offset.max(0)"), "{storage}");
}

#[test]
fn save_many_is_a_single_unnest_insert() {
    let storage = rating_storage();
    assert!(
        storage.contains("INSERT INTO ratings (spot_id, comment) SELECT * FROM UNNEST($1::BIGINT[], $2::TEXT[]) RETURNING id, spot_id, comment"),
        "{storage}"
    );
    assert!(!storage.contains("self.save_one(item)"), "{storage}");
}

#[test]
fn filtered_and_permission_columns_are_indexed() {
    let project = generate(RATINGS_TR);
    let ratings = project.read("main/migrations/ratings.sql");
    assert!(
        ratings.contains("CREATE INDEX IF NOT EXISTS ratings_spot_id_id_idx ON ratings (spot_id, id);"),
        "{ratings}"
    );
    let permissions = project.read("main/migrations/permissions.sql");
    assert!(
        permissions.contains("ON permissions (user_id, target, action, id);"),
        "{permissions}"
    );
}

#[test]
fn psql_pool_size_is_configurable_with_a_default() {
    let main = generate(RATINGS_TR).read("main/src/main.rs");
    assert!(main.contains(r#"std::env::var("RATING_INFRA_DB_POOL_SIZE")"#), "{main}");
    assert!(main.contains("Err(_) => 16,"), "{main}");
    assert!(
        main.contains("SQLConnector::connect_with_pool_size(&rating_infra_db_url, rating_infra_db_pool_size)"),
        "{main}"
    );
}

#[test]
fn dev_env_scaffolds_the_pool_size() {
    let asts = support::parse(RATINGS_TR);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("out");
    trust::generator::CodeGen::new(asts)
        .run(root.clone(), true, &trust::ai::AiConfig::new_for_testing("http://127.0.0.1:1"))
        .unwrap();
    let env = std::fs::read_to_string(root.join("main/.env")).unwrap();
    assert!(env.contains("RATING_INFRA_DB_POOL_SIZE=16"), "{env}");
}

#[test]
fn workspace_release_profile_optimises_for_runtime() {
    let cargo = generate(RATINGS_TR).read("Cargo.toml");
    assert!(cargo.contains("[profile.release]"), "{cargo}");
    assert!(cargo.contains(r#"lto = "fat""#), "{cargo}");
    assert!(cargo.contains("codegen-units = 1"), "{cargo}");
    assert!(!cargo.contains("panic = \"abort\""), "{cargo}");
}

#[test]
fn load_balancer_streams_bodies_instead_of_buffering_them() {
    let project = generate(
        r#"app Balancer: run {
            load_balancer LB {
                targets {
                    "10.0.0.1:8080"
                }
                on: "0.0.0.0:9000"
            }
        }"#,
    );
    let proxy = project.read("balancer/src/adapters/http/load_balancer.rs");
    assert!(!proxy.contains("to_bytes("), "{proxy}");
    assert!(proxy.contains("reqwest::Body::wrap_stream(body.into_data_stream())"), "{proxy}");
    assert!(proxy.contains("Body::from_stream(backend_response.bytes_stream())"), "{proxy}");
    let cargo = project.read("balancer/Cargo.toml");
    assert!(cargo.contains(r#"reqwest = { version = "0.12", features = ["stream"] }"#), "{cargo}");
}
