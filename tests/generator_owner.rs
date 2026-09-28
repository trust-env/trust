#[path = "support/mod.rs"]
mod support;

use support::{generate, try_generate};

const CART_TR: &str = r#"type CartItem {
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

use_case ListPublicStickersUseCase {
    call: StickerInfra.list
    filters {
        visibility: "community"
    }
}

adapter HttpApi {
    http {
        on: "0.0.0.0:8080"
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
            GET "/stickers" {
                use_case: ListPublicStickersUseCase
            }
        }
    }
}

app Main: run {
    HttpApi
}
"#;

fn use_case(file: &str) -> String {
    generate(CART_TR).read(&format!("main/src/applications/{file}.rs"))
}

#[test]
fn owner_field_is_never_part_of_a_guessed_input() {
    let code = use_case("add_cart_item_use_case");
    let input = code
        .split("pub struct AddCartItemUseCaseInput")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(
        !input.contains("user_id"),
        "owner must not be client-supplied:\n{code}"
    );
    assert!(input.contains("sticker_id"), "{code}");
}

#[test]
fn save_one_stamps_the_owner_from_the_caller() {
    let code = use_case("add_cart_item_use_case");
    assert!(code.contains("async fn execute(&self, user: i64"), "{code}");
    assert!(code.contains("entity.user_id = user;"), "{code}");
}

#[test]
fn list_is_filtered_to_the_caller() {
    let code = use_case("list_cart_use_case");
    assert!(
        code.contains(r#"StoragePredicate { field: "user_id".to_string(), operator: StoragePredicateOperator::Eq, value: user.to_string() }"#),
        "{code}"
    );
    assert!(
        code.contains(
            "use common::{UseCase, StorageListQuery, StoragePredicate, StoragePredicateOperator};"
        ),
        "the list use case must import the predicate types:\n{code}"
    );
}

#[test]
fn find_one_hides_rows_owned_by_someone_else() {
    let code = use_case("get_cart_item_use_case");
    assert!(
        code.contains(".filter(|entity| entity.user_id == user)"),
        "{code}"
    );
    assert!(
        code.contains(".ok_or(GetCartItemUseCaseError::NotFound)"),
        "{code}"
    );
}

#[test]
fn update_and_delete_check_ownership_before_writing() {
    for (file, error) in [
        ("update_cart_item_use_case", "UpdateCartItemUseCaseError"),
        ("remove_cart_item_use_case", "RemoveCartItemUseCaseError"),
    ] {
        let code = use_case(file);
        let guard = code
            .find(".filter(|existing| existing.user_id == user)")
            .unwrap_or_else(|| panic!("no guard:\n{code}"));
        let write = code
            .rfind(".update_one(")
            .or_else(|| code.rfind(".delete("))
            .unwrap();
        assert!(guard < write, "guard must run before the write:\n{code}");
        assert!(
            code.contains(&format!(".ok_or({error}::NotFound)?;")),
            "{code}"
        );
    }
    assert!(use_case("update_cart_item_use_case").contains("entity.user_id = user;"));
}

#[test]
fn literal_filters_compare_against_a_fixed_value_and_add_no_input_field() {
    let code = use_case("list_public_stickers_use_case");
    assert!(code.contains(r#"field: "visibility".to_string(), operator: StoragePredicateOperator::Eq, value: "community".to_string()"#), "{code}");
    let input = code
        .split("pub struct ListPublicStickersUseCaseInput")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(!input.contains("visibility"), "{code}");
    assert!(
        code.contains("_user: i64"),
        "unowned use cases keep ignoring the user:\n{code}"
    );
}

#[test]
fn an_owner_use_case_behind_a_route_without_auth_is_rejected() {
    let src = CART_TR.replacen(
        "            GET \"/cart\" {\n                use_case: ListCartUseCase\n                auth: required\n",
        "            GET \"/cart\" {\n                use_case: ListCartUseCase\n",
        1,
    );
    let err = try_generate(&src).unwrap_err();
    assert!(err.contains("must declare 'auth'"), "{err}");
}

#[test]
fn owner_must_name_an_int_field_on_the_entity() {
    let unknown = CART_TR.replacen(
        "call: CartInfra.list\n    owner: user_id",
        "call: CartInfra.list\n    owner: nobody",
        1,
    );
    assert!(
        try_generate(&unknown)
            .unwrap_err()
            .contains("unknown field 'nobody'")
    );

    let wrong_type = CART_TR.replacen("user_id: Int,", "user_id: String,", 1);
    assert!(
        try_generate(&wrong_type)
            .unwrap_err()
            .contains("must be 'Int'")
    );
}
