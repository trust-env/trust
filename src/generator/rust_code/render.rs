use std::sync::{LazyLock, RwLock};

use tera::{Context, Tera};

/// Every generated-code template, embedded into the binary at compile time
/// (`include_str!`) so there's no runtime dependency on a `templates/`
/// directory being present next to the executable. Rust resolves all the
/// *data* (what a use case's input type is, which infra a call targets,
/// ...); these templates only ever decide the text *layout* of that data.
///
/// Wrapped in an `RwLock` (rather than a plain `Tera`) so that plugin-
/// supplied templates can be added at runtime, after this static's one-time
/// lazy init, via `register_plugin_templates` — every bundled template
/// above is still resolved purely at compile time.
static TEMPLATES: LazyLock<RwLock<Tera>> = LazyLock::new(|| {
    let mut tera = Tera::default();
    tera.add_raw_templates(vec![
        ("types.rs.tera", include_str!("../../../templates/types.rs.tera")),
        ("_trait/trait.rs.tera", include_str!("../../../templates/_trait/trait.rs.tera")),
        ("_trait/trait_impl.rs.tera", include_str!("../../../templates/_trait/trait_impl.rs.tera")),
        ("service/method.rs.tera", include_str!("../../../templates/service/method.rs.tera")),
        ("service/service.rs.tera", include_str!("../../../templates/service/service.rs.tera")),
        ("file/mod_file.rs.tera", include_str!("../../../templates/file/mod_file.rs.tera")),
        ("cargo_toml.toml.tera", include_str!("../../../templates/cargo_toml.toml.tera")),
        (
            "workspace_cargo_toml.toml.tera",
            include_str!("../../../templates/workspace_cargo_toml.toml.tera"),
        ),
        ("proto/message.proto.tera", include_str!("../../../templates/proto/message.proto.tera")),
        (
            "proto/wrapper_message.proto.tera",
            include_str!("../../../templates/proto/wrapper_message.proto.tera"),
        ),
        (
            "service/psql_storage/save_one.rs.tera",
            include_str!("../../../templates/service/psql_storage/save_one.rs.tera"),
        ),
        (
            "service/psql_storage/find_one.rs.tera",
            include_str!("../../../templates/service/psql_storage/find_one.rs.tera"),
        ),
        (
            "service/psql_storage/find_many.rs.tera",
            include_str!("../../../templates/service/psql_storage/find_many.rs.tera"),
        ),
        (
            "service/psql_storage/list.rs.tera",
            include_str!("../../../templates/service/psql_storage/list.rs.tera"),
        ),
        (
            "service/psql_storage/update_one.rs.tera",
            include_str!("../../../templates/service/psql_storage/update_one.rs.tera"),
        ),
        (
            "service/psql_storage/delete.rs.tera",
            include_str!("../../../templates/service/psql_storage/delete.rs.tera"),
        ),
        (
            "service/psql_storage/delete_many.rs.tera",
            include_str!("../../../templates/service/psql_storage/delete_many.rs.tera"),
        ),
        (
            "file/psql_storage/error_enum.rs.tera",
            include_str!("../../../templates/file/psql_storage/error_enum.rs.tera"),
        ),
        (
            "file/ai_use_case/error.rs.tera",
            include_str!("../../../templates/file/ai_use_case/error.rs.tera"),
        ),
        (
            "file/ai_use_case/trait.rs.tera",
            include_str!("../../../templates/file/ai_use_case/trait.rs.tera"),
        ),
        (
            "file/ai_use_case/struct.rs.tera",
            include_str!("../../../templates/file/ai_use_case/struct.rs.tera"),
        ),
        (
            "file/ai_use_case/impl.rs.tera",
            include_str!("../../../templates/file/ai_use_case/impl.rs.tera"),
        ),
        ("domains/derive.rs.tera", include_str!("../../../templates/domains/derive.rs.tera")),
        (
            "dev_docs/fields_table.html.tera",
            include_str!("../../../templates/dev_docs/fields_table.html.tera"),
        ),
        (
            "dev_docs/errors_table.html.tera",
            include_str!("../../../templates/dev_docs/errors_table.html.tera"),
        ),
        (
            "dev_docs/route_section.html.tera",
            include_str!("../../../templates/dev_docs/route_section.html.tera"),
        ),
        ("dev_docs/page.html.tera", include_str!("../../../templates/dev_docs/page.html.tera")),
        ("file/main/infra.rs.tera", include_str!("../../../templates/file/main/infra.rs.tera")),
        (
            "file/main/grpc_client_infra.rs.tera",
            include_str!("../../../templates/file/main/grpc_client_infra.rs.tera"),
        ),
        ("file/main/use_case.rs.tera", include_str!("../../../templates/file/main/use_case.rs.tera")),
        ("file/main/adapter.rs.tera", include_str!("../../../templates/file/main/adapter.rs.tera")),
        (
            "file/main/grpc_adapter.rs.tera",
            include_str!("../../../templates/file/main/grpc_adapter.rs.tera"),
        ),
        (
            "file/main/load_balancer.rs.tera",
            include_str!("../../../templates/file/main/load_balancer.rs.tera"),
        ),
        (
            "file/use_case/input_struct.rs.tera",
            include_str!("../../../templates/file/use_case/input_struct.rs.tera"),
        ),
        (
            "file/use_case/entity_conversion.rs.tera",
            include_str!("../../../templates/file/use_case/entity_conversion.rs.tera"),
        ),
        (
            "file/use_case/output_struct.rs.tera",
            include_str!("../../../templates/file/use_case/output_struct.rs.tera"),
        ),
        ("file/use_case/error.rs.tera", include_str!("../../../templates/file/use_case/error.rs.tera")),
        ("file/use_case/struct.rs.tera", include_str!("../../../templates/file/use_case/struct.rs.tera")),
        (
            "file/use_case/call_simple.rs.tera",
            include_str!("../../../templates/file/use_case/call_simple.rs.tera"),
        ),
        (
            "file/use_case/call_read.rs.tera",
            include_str!("../../../templates/file/use_case/call_read.rs.tera"),
        ),
        (
            "file/http_adapter/struct.rs.tera",
            include_str!("../../../templates/file/http_adapter/struct.rs.tera"),
        ),
        (
            "file/http_adapter/ctor.rs.tera",
            include_str!("../../../templates/file/http_adapter/ctor.rs.tera"),
        ),
        (
            "file/http_adapter/router.rs.tera",
            include_str!("../../../templates/file/http_adapter/router.rs.tera"),
        ),
        (
            "file/http_adapter/split_structs.rs.tera",
            include_str!("../../../templates/file/http_adapter/split_structs.rs.tera"),
        ),
        (
            "file/http_adapter/auth_prelude.rs.tera",
            include_str!("../../../templates/file/http_adapter/auth_prelude.rs.tera"),
        ),
        (
            "file/http_adapter/error_status_fn.rs.tera",
            include_str!("../../../templates/file/http_adapter/error_status_fn.rs.tera"),
        ),
        (
            "file/http_adapter/handler.rs.tera",
            include_str!("../../../templates/file/http_adapter/handler.rs.tera"),
        ),
        (
            "file/http_adapter/app_docs.rs.tera",
            include_str!("../../../templates/file/http_adapter/app_docs.rs.tera"),
        ),
        (
            "file/grpc_adapter/proto.proto.tera",
            include_str!("../../../templates/file/grpc_adapter/proto.proto.tera"),
        ),
        (
            "file/grpc_adapter/output_construction.rs.tera",
            include_str!("../../../templates/file/grpc_adapter/output_construction.rs.tera"),
        ),
        (
            "file/grpc_adapter/rpc_method.rs.tera",
            include_str!("../../../templates/file/grpc_adapter/rpc_method.rs.tera"),
        ),
        (
            "file/grpc_adapter/service_impl.rs.tera",
            include_str!("../../../templates/file/grpc_adapter/service_impl.rs.tera"),
        ),
        (
            "file/grpc_client_storage/client.rs.tera",
            include_str!("../../../templates/file/grpc_client_storage/client.rs.tera"),
        ),
        (
            "file/grpc_client_storage/find_one_method.rs.tera",
            include_str!("../../../templates/file/grpc_client_storage/find_one_method.rs.tera"),
        ),
        (
            "file/grpc_client_storage/unsupported_method.rs.tera",
            include_str!("../../../templates/file/grpc_client_storage/unsupported_method.rs.tera"),
        ),
        (
            "file/s3_storage/storage.rs.tera",
            include_str!("../../../templates/file/s3_storage/storage.rs.tera"),
        ),
        (
            "file/main/s3_infra.rs.tera",
            include_str!("../../../templates/file/main/s3_infra.rs.tera"),
        ),
        (
            "file/blob_use_case/output_struct.rs.tera",
            include_str!("../../../templates/file/blob_use_case/output_struct.rs.tera"),
        ),
        (
            "file/blob_use_case/struct.rs.tera",
            include_str!("../../../templates/file/blob_use_case/struct.rs.tera"),
        ),
        (
            "file/blob_use_case/impl.rs.tera",
            include_str!("../../../templates/file/blob_use_case/impl.rs.tera"),
        ),
    ])
    .expect("all bundled templates should be valid Tera syntax");
    RwLock::new(tera)
});

/// Renders `name` (a key registered above) against `context`, panicking with
/// the template name and Tera's own diagnostic on failure — a bundled
/// template failing to render is always a bug in this compiler, never
/// user-facing input.
pub fn render(name: &str, context: &Context) -> String {
    TEMPLATES
        .read()
        .unwrap_or_else(|e| panic!("template registry lock poisoned: {e}"))
        .render(name, context)
        .unwrap_or_else(|e| panic!("template '{}' failed to render: {}", name, e))
}

/// Same as `render`, but returns a `Result` instead of panicking. Used
/// exclusively by the plugin codegen path (`generator::plugin`), since a
/// plugin-supplied template failing to render is never a bug in this
/// compiler the way a bundled template failing would be.
pub fn try_render(name: &str, context: &Context) -> Result<String, String> {
    TEMPLATES
        .read()
        .map_err(|e| format!("template registry lock poisoned: {e}"))?
        .render(name, context)
        .map_err(|e| format!("template '{}' failed to render: {}", name, e))
}

/// Registers plugin-supplied `(name, content)` template pairs into the
/// shared registry, under whatever name the caller passes (callers should
/// namespace plugin template names, e.g. `plugin/<plugin_name>/<name>`, to
/// avoid colliding with a bundled template or another plugin's).
pub fn register_plugin_templates(templates: &[(String, String)]) -> Result<(), String> {
    let mut tera = TEMPLATES.write().map_err(|e| format!("template registry lock poisoned: {e}"))?;
    for (name, content) in templates {
        tera.add_raw_template(name, content)
            .map_err(|e| format!("plugin template '{}' is not valid Tera syntax: {}", name, e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_template_is_registered_and_parses() {
        // `TEMPLATES` is built eagerly here (rather than relying on some
        // other test to touch it first) so a syntax error in any bundled
        // template surfaces as a clear, named test failure.
        let tera = TEMPLATES.read().unwrap();
        let mut names: Vec<&str> = tera.get_template_names().collect();
        names.sort();
        assert!(names.contains(&"types.rs.tera"));
        assert!(names.contains(&"_trait/trait.rs.tera"));
        assert!(names.contains(&"_trait/trait_impl.rs.tera"));
        assert!(names.contains(&"service/method.rs.tera"));
        assert!(names.contains(&"service/service.rs.tera"));
        assert!(names.contains(&"file/mod_file.rs.tera"));
        assert!(names.contains(&"cargo_toml.toml.tera"));
        assert!(names.contains(&"workspace_cargo_toml.toml.tera"));
        assert!(names.contains(&"proto/message.proto.tera"));
        assert!(names.contains(&"proto/wrapper_message.proto.tera"));
        assert!(names.contains(&"service/psql_storage/save_one.rs.tera"));
        assert!(names.contains(&"service/psql_storage/find_one.rs.tera"));
        assert!(names.contains(&"service/psql_storage/find_many.rs.tera"));
        assert!(names.contains(&"service/psql_storage/list.rs.tera"));
        assert!(names.contains(&"service/psql_storage/update_one.rs.tera"));
        assert!(names.contains(&"service/psql_storage/delete.rs.tera"));
        assert!(names.contains(&"service/psql_storage/delete_many.rs.tera"));
        assert!(names.contains(&"file/psql_storage/error_enum.rs.tera"));
        assert!(names.contains(&"file/ai_use_case/error.rs.tera"));
        assert!(names.contains(&"file/ai_use_case/trait.rs.tera"));
        assert!(names.contains(&"file/ai_use_case/struct.rs.tera"));
        assert!(names.contains(&"file/ai_use_case/impl.rs.tera"));
        assert!(names.contains(&"domains/derive.rs.tera"));
        assert!(names.contains(&"dev_docs/fields_table.html.tera"));
        assert!(names.contains(&"dev_docs/errors_table.html.tera"));
        assert!(names.contains(&"dev_docs/route_section.html.tera"));
        assert!(names.contains(&"dev_docs/page.html.tera"));
        assert!(names.contains(&"file/main/infra.rs.tera"));
        assert!(names.contains(&"file/main/grpc_client_infra.rs.tera"));
        assert!(names.contains(&"file/main/use_case.rs.tera"));
        assert!(names.contains(&"file/main/adapter.rs.tera"));
        assert!(names.contains(&"file/main/grpc_adapter.rs.tera"));
        assert!(names.contains(&"file/main/load_balancer.rs.tera"));
        assert!(names.contains(&"file/use_case/input_struct.rs.tera"));
        assert!(names.contains(&"file/use_case/entity_conversion.rs.tera"));
        assert!(names.contains(&"file/use_case/output_struct.rs.tera"));
        assert!(names.contains(&"file/use_case/error.rs.tera"));
        assert!(names.contains(&"file/use_case/struct.rs.tera"));
        assert!(names.contains(&"file/use_case/call_simple.rs.tera"));
        assert!(names.contains(&"file/use_case/call_read.rs.tera"));
        assert!(names.contains(&"file/http_adapter/struct.rs.tera"));
        assert!(names.contains(&"file/http_adapter/ctor.rs.tera"));
        assert!(names.contains(&"file/http_adapter/router.rs.tera"));
        assert!(names.contains(&"file/http_adapter/split_structs.rs.tera"));
        assert!(names.contains(&"file/http_adapter/auth_prelude.rs.tera"));
        assert!(names.contains(&"file/http_adapter/error_status_fn.rs.tera"));
        assert!(names.contains(&"file/http_adapter/handler.rs.tera"));
        assert!(names.contains(&"file/http_adapter/app_docs.rs.tera"));
        assert!(names.contains(&"file/grpc_adapter/proto.proto.tera"));
        assert!(names.contains(&"file/grpc_adapter/output_construction.rs.tera"));
        assert!(names.contains(&"file/grpc_adapter/rpc_method.rs.tera"));
        assert!(names.contains(&"file/grpc_adapter/service_impl.rs.tera"));
        assert!(names.contains(&"file/grpc_client_storage/client.rs.tera"));
        assert!(names.contains(&"file/grpc_client_storage/find_one_method.rs.tera"));
        assert!(names.contains(&"file/grpc_client_storage/unsupported_method.rs.tera"));
        assert!(names.contains(&"file/s3_storage/storage.rs.tera"));
        assert!(names.contains(&"file/main/s3_infra.rs.tera"));
        assert!(names.contains(&"file/blob_use_case/output_struct.rs.tera"));
        assert!(names.contains(&"file/blob_use_case/struct.rs.tera"));
        assert!(names.contains(&"file/blob_use_case/impl.rs.tera"));
    }
}
