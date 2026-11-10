#[path = "support/mod.rs"]
mod support;

use support::generate;

const RATINGS_TR: &str = r#"type Rating {
    id: Int,
    spot_id: Int,
    temperature: Int,
    safety: Int
}

infra RatingInfra {
    storage {
        psql {
            entity: Rating
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
            }
        }
    }
}

app Main: run {
    HttpApi
}"#;

#[test]
fn filtered_list_input_gains_the_filter_field_alongside_limit_and_offset() {
    let project = generate(RATINGS_TR);

    let uc = project.read("main/src/applications/list_ratings_for_spot_use_case.rs");
    assert!(uc.contains("pub struct ListRatingsForSpotUseCaseInput {"));
    // Paging is optional: absent means the defaults.
    assert!(uc.contains("pub limit: Option<i64>,"));
    assert!(uc.contains("pub offset: Option<i64>,"));
    assert!(uc.contains("pub spot_id: i64,"));
}

#[test]
fn filtered_list_builds_an_equality_predicate_against_the_input_field() {
    let project = generate(RATINGS_TR);

    let uc = project.read("main/src/applications/list_ratings_for_spot_use_case.rs");
    assert!(uc.contains(
        "StoragePredicate { field: \"spot_id\".to_string(), operator: StoragePredicateOperator::Eq, value: input.spot_id.to_string() }"
    ));
    assert!(uc.contains(
        "use common::{UseCase, StorageListQuery, StoragePredicate, StoragePredicateOperator};"
    ));
}

#[test]
fn spot_id_path_param_routes_to_the_path_extractor_not_the_body() {
    // The filter field must be reachable from the route's `:spot_id` path
    // segment via the existing (unmodified) path/body-splitting mechanism —
    // this only works if `spot_id` actually landed on the guessed input.
    let project = generate(RATINGS_TR);

    let handler =
        project.read("main/src/adapters/http/http_api/list_ratings_for_spot_use_case_get.rs");
    assert!(handler.contains(
        "pub(crate) struct ListRatingsForSpotUseCasePathParams {\n    pub spot_id: i64,\n}"
    ));
    assert!(handler.contains(
        "pub(crate) struct ListRatingsForSpotUseCaseBody {\n    pub limit: Option<i64>,\n    pub offset: Option<i64>,\n}"
    ));
    assert!(handler.contains("spot_id: path.spot_id,"));
    // GET routes read their non-path fields from the query string, not a
    // JSON body — a real HTTP client refuses to send a body on GET at all.
    assert!(handler.contains("Query(body): Query<ListRatingsForSpotUseCaseBody>,"));
    assert!(!handler.contains("Json(body)"));
}

#[test]
fn use_case_without_a_filters_block_still_gets_an_empty_predicates_list() {
    let project = generate(
        r#"type Task {
            id: Int,
            title: String
        }
        infra TaskInfra {
            storage {
                psql {
                    entity: Task
                }
            }
        }
        use_case ListTasksUseCase {
            call: TaskInfra.list
        }"#,
    );

    let uc = project.read("src/applications/list_tasks_use_case.rs");
    assert!(uc.contains("predicates: vec![],"));
    assert!(!uc.contains("StoragePredicate"));
}
