use crate::generator::rust_code::{file::http_adapter_file::HttpRouteBinding, render::render as render_template};
use serde::Serialize;
use tera::Context;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Serialize)]
struct FieldView {
    name: String,
    field_type: String,
}

fn fields_table(fields: &[(String, String)]) -> String {
    let views: Vec<FieldView> = fields
        .iter()
        .map(|(name, ty)| FieldView {
            name: escape(name),
            field_type: escape(ty),
        })
        .collect();

    let mut context = Context::new();
    context.insert("fields", &views);
    render_template("dev_docs/fields_table.html.tera", &context)
}

#[derive(Serialize)]
struct ErrorRowView {
    target: String,
    status: u16,
    message: String,
}

fn errors_table(route: &HttpRouteBinding) -> String {
    let target_status = |target: &str| -> u16 {
        route
            .status_mappings
            .iter()
            .find(|m| m.target == target)
            .or_else(|| route.status_mappings.iter().find(|m| m.target == "_"))
            .map(|m| m.status)
            .unwrap_or(500)
    };

    let mut rows = vec![
        ErrorRowView {
            target: "NotFound".to_string(),
            status: target_status("NotFound"),
            message: String::new(),
        },
        ErrorRowView {
            target: "StorageError".to_string(),
            status: target_status("StorageError"),
            message: String::new(),
        },
    ];

    let mut seen = Vec::new();
    for mapping in &route.error_mappings {
        if seen.contains(&mapping.target.as_str()) {
            continue;
        }
        seen.push(mapping.target.as_str());
        rows.push(ErrorRowView {
            target: escape(&mapping.target),
            status: target_status(&mapping.target),
            message: mapping.message.as_deref().map(escape).unwrap_or_default(),
        });
    }

    let mut context = Context::new();
    context.insert("rows", &rows);
    render_template("dev_docs/errors_table.html.tera", &context)
}

fn route_section(route: &HttpRouteBinding) -> String {
    let output_type = if route.output_is_list {
        format!("Vec&lt;{}&gt;", escape(&route.output_entity))
    } else {
        escape(&route.output_entity)
    };

    let path_params_joined = route
        .path_params
        .iter()
        .map(|p| format!("<code>{}</code>", escape(p)))
        .collect::<Vec<_>>()
        .join(", ");

    let mut context = Context::new();
    context.insert("method_class", route.method.as_axum_fn());
    context.insert("method", &route.method.as_axum_fn().to_uppercase());
    context.insert("path", &escape(&route.path));
    context.insert("use_case", &escape(&route.use_case_name));
    context.insert("path_params_joined", &path_params_joined);
    context.insert("input_table", &fields_table(&route.input_fields));
    context.insert("output_type", &output_type);
    context.insert("output_table", &fields_table(&route.output_fields));
    context.insert("errors_table", &errors_table(route));
    render_template("dev_docs/route_section.html.tera", &context)
}

/// Renders a single self-contained HTML page documenting every route this
/// adapter exposes: its request/response shape and every error it can
/// produce, with the HTTP status each one maps to.
pub fn render(adapter_name: &str, routes: &[HttpRouteBinding]) -> String {
    let sections: Vec<String> = routes.iter().map(route_section).collect();

    let mut context = Context::new();
    context.insert("adapter_name", &escape(adapter_name));
    context.insert("sections", &sections);
    render_template("dev_docs/page.html.tera", &context)
}
