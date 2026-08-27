use crate::generator::rust_code::render::render;
use serde::Serialize;
use tera::Context;

/// Maps a scalar Rust type name (as it appears in `UseCaseIo`'s field docs)
/// to its proto3 type. gRPC-exposed use cases only support flat scalar
/// fields for now — `Uuid`, `Vec<...>` (list outputs), and nested struct
/// fields aren't supported yet, so anything else is a clear error at
/// generation time rather than invalid `.proto` output.
pub fn proto_scalar_type(rust_type: &str) -> Result<&'static str, String> {
    match rust_type {
        "String" => Ok("string"),
        "i64" => Ok("int64"),
        other => Err(format!(
            "gRPC doesn't support field type '{}' yet — only String and Int fields are supported",
            other
        )),
    }
}

/// Renders one `message` block from a use case's field docs (name, Rust
/// type string) — the same summary already used for `--dev` HTTP docs.
pub fn render_message(name: &str, fields: &[(String, String)]) -> Result<String, String> {
    #[derive(Serialize)]
    struct FieldView {
        proto_type: &'static str,
        field_name: String,
        number: usize,
    }

    let mut field_views = Vec::with_capacity(fields.len());
    for (i, (field_name, field_type)) in fields.iter().enumerate() {
        field_views.push(FieldView {
            proto_type: proto_scalar_type(field_type)?,
            field_name: field_name.clone(),
            number: i + 1,
        });
    }

    let mut context = Context::new();
    context.insert("name", name);
    context.insert("fields", &field_views);
    Ok(render("proto/message.proto.tera", &context))
}

/// Renders a one-field `message` whose single field references another
/// message by name (a proto "message-typed" field), rather than a scalar —
/// used for CRUD use cases' output wrapper (e.g. `CreateUseCaseOutput {
/// User user = 1; }`).
pub fn render_wrapper_message(name: &str, field_name: &str, entity_message_name: &str) -> String {
    let mut context = Context::new();
    context.insert("name", name);
    context.insert("field_name", field_name);
    context.insert("entity_message_name", entity_message_name);
    render("proto/wrapper_message.proto.tera", &context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_scalar_type_supports_string_and_i64() {
        assert_eq!(proto_scalar_type("String").unwrap(), "string");
        assert_eq!(proto_scalar_type("i64").unwrap(), "int64");
    }

    #[test]
    fn proto_scalar_type_rejects_uuid_and_other_unsupported_types() {
        // Pins the current gRPC scalar-only limitation: Uuid and any other
        // non-scalar type is a clear generation-time error, not silently
        // emitted as invalid `.proto`.
        let err = proto_scalar_type("Uuid").unwrap_err();
        assert!(err.contains("Uuid"));
        assert!(err.contains("only String and Int"));

        let err = proto_scalar_type("Vec<String>").unwrap_err();
        assert!(err.contains("Vec<String>"));
    }

    #[test]
    fn proto_scalar_type_rejects_float_and_decimal() {
        // Float/Decimal are HTTP+psql only for now, same gap as Uuid — this
        // pins it as deliberate rather than an oversight if gRPC support is
        // ever added for either.
        let err = proto_scalar_type("f64").unwrap_err();
        assert!(err.contains("f64"));

        let err = proto_scalar_type("Decimal").unwrap_err();
        assert!(err.contains("Decimal"));
    }

    #[test]
    fn render_message_produces_one_numbered_field_per_entry() {
        let fields = vec![
            ("id".to_string(), "String".to_string()),
            ("age".to_string(), "i64".to_string()),
        ];
        let rendered = render_message("User", &fields).unwrap();
        assert_eq!(
            rendered,
            "message User {\n    string id = 1;\n    int64 age = 2;\n}\n\n"
        );
    }

    #[test]
    fn render_message_propagates_unsupported_field_type_error() {
        let fields = vec![("id".to_string(), "Uuid".to_string())];
        assert!(render_message("User", &fields).is_err());
    }

    #[test]
    fn render_wrapper_message_wraps_the_entity_type_by_name() {
        // Pins the CRUD-output-wrapper shape: the field's proto type is the
        // referenced message name itself, not a scalar.
        let rendered = render_wrapper_message("CreateUserOutput", "user", "User");
        assert_eq!(rendered, "message CreateUserOutput {\n    User user = 1;\n}\n\n");
    }
}
