//! JSON Schema and TypeScript renderings of resolved types — for MCP tool
//! schemas and the `--emit-ts` / `--emit-openapi` outputs. Both follow the
//! wire format: `Option` fields may be missing or null, `Decimal` is a
//! string, dates are ISO strings, enums are their wire values, and field
//! names are camelCase under `config { json_case: "camel" }`.

use heck::ToLowerCamelCase;
use serde_json::{Map, Value, json};

use crate::generator::{
    rust_code::{
        imports::DATE_TIME,
        types::{RustType, TypeKind},
    },
    types::JSON_VALUE,
};

/// A field's name on the wire.
pub fn wire_name(name: &str, camel: bool) -> String {
    if camel { name.to_lower_camel_case() } else { name.to_string() }
}

/// JSON Schema of `t`. Declared structs are inlined (MCP tool schemas
/// must be self-contained).
pub fn json_schema(t: &RustType, camel: bool) -> Value {
    if let TypeKind::Enum(values) = &t.kind {
        return json!({ "type": "string", "enum": values });
    }
    if let TypeKind::TaggedEnum(variants) = &t.kind {
        let options: Vec<Value> = variants
            .iter()
            .map(|(wire, fields)| {
                let mut schema = object_schema(&RustType::generic(&t.name, vec![], fields.clone()), camel);
                schema["properties"]["type"] = json!({ "const": wire });
                let mut required = schema["required"].as_array().cloned().unwrap_or_default();
                required.insert(0, json!("type"));
                schema["required"] = Value::Array(required);
                schema
            })
            .collect();
        return json!({ "oneOf": options });
    }
    match t.name.as_str() {
        "Option" => {
            let inner = t.templates.first().map(|i| json_schema(i, camel)).unwrap_or(json!({}));
            json!({ "anyOf": [inner, { "type": "null" }] })
        }
        "Vec" => {
            let items = t.templates.first().map(|i| json_schema(i, camel)).unwrap_or(json!({}));
            json!({ "type": "array", "items": items })
        }
        "i64" | "f64" => {
            let mut schema = json!({ "type": if t.name == "i64" { "integer" } else { "number" } });
            if let TypeKind::Range(min, max) = &t.kind {
                if let Some(min) = min.as_ref().and_then(|m| m.parse::<f64>().ok()) {
                    schema["minimum"] = json!(min);
                }
                if let Some(max) = max.as_ref().and_then(|m| m.parse::<f64>().ok()) {
                    schema["maximum"] = json!(max);
                }
            }
            schema
        }
        "bool" => json!({ "type": "boolean" }),
        "String" => match &t.kind {
            TypeKind::Regex(pattern) => json!({ "type": "string", "pattern": pattern }),
            _ => json!({ "type": "string" }),
        },
        "Uuid" => json!({ "type": "string", "format": "uuid" }),
        "Decimal" => json!({ "type": "string", "pattern": "^-?[0-9]+(\\.[0-9]+)?$" }),
        DATE_TIME => json!({ "type": "string", "format": "date-time" }),
        "NaiveDate" => json!({ "type": "string", "format": "date" }),
        "NaiveTime" => json!({ "type": "string", "format": "time" }),
        JSON_VALUE => json!({}),
        "HashMap<String, String>" => json!({ "type": "object", "additionalProperties": { "type": "string" } }),
        _ => object_schema(t, camel),
    }
}

fn object_schema(t: &RustType, camel: bool) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for field in &t.fields {
        let name = wire_name(&field.name, camel);
        if !field.field_type.is_option() {
            required.push(Value::String(name.clone()));
        }
        properties.insert(name, json_schema(&field.field_type, camel));
    }
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".into(), Value::Array(required));
    }
    Value::Object(schema)
}

/// A TypeScript type expression for `t`, naming declared structs and enums
/// (their own declarations come from `ts_declaration`).
pub fn ts_type(t: &RustType) -> String {
    if t.is_enum() || t.kind == TypeKind::Struct || matches!(t.kind, TypeKind::TaggedEnum(_)) {
        return t.name.clone();
    }
    match t.name.as_str() {
        "Option" => {
            let inner = t.templates.first().map(ts_type).unwrap_or_else(|| "unknown".into());
            if inner.ends_with(" | null") { inner } else { format!("{} | null", inner) }
        }
        "Vec" => {
            let inner = t.templates.first().map(ts_type).unwrap_or_else(|| "unknown".into());
            if inner.contains(' ') { format!("Array<{}>", inner) } else { format!("{}[]", inner) }
        }
        "i64" | "f64" => "number".into(),
        "bool" => "boolean".into(),
        "String" | "Uuid" | "Decimal" | DATE_TIME | "NaiveDate" | "NaiveTime" => "string".into(),
        JSON_VALUE => "unknown".into(),
        "HashMap<String, String>" => "Record<string, string>".into(),
        _ if !t.fields.is_empty() => t.name.clone(),
        other => other.to_string(),
    }
}

/// `export interface X { ... }` / `export type E = "a" | "b";`.
pub fn ts_declaration(t: &RustType, camel: bool) -> String {
    if let TypeKind::Enum(values) = &t.kind {
        let values: Vec<String> = values.iter().map(|v| format!("{:?}", v)).collect();
        return format!("export type {} = {};\n", t.name, values.join(" | "));
    }
    if let TypeKind::TaggedEnum(variants) = &t.kind {
        let options: Vec<String> = variants
            .iter()
            .map(|(wire, fields)| {
                let mut members = vec![format!("type: {:?}", wire)];
                for field in fields {
                    let optional = if field.field_type.is_option() { "?" } else { "" };
                    members.push(format!("{}{}: {}", wire_name(&field.name, camel), optional, ts_type(&field.field_type)));
                }
                format!("{{ {} }}", members.join("; "))
            })
            .collect();
        return format!("export type {} =\n  | {};\n", t.name, options.join("\n  | "));
    }
    let mut out = format!("export interface {} {{\n", t.name);
    for field in &t.fields {
        let optional = if field.field_type.is_option() { "?" } else { "" };
        out.push_str(&format!(
            "  {}{}: {};\n",
            wire_name(&field.name, camel),
            optional,
            ts_type(&field.field_type)
        ));
    }
    out.push_str("}\n");
    out
}

/// Every declared struct / enum `t` mentions (itself included), depth
/// first, each once.
pub fn collect_named_types(t: &RustType, out: &mut Vec<RustType>) {
    let named = t.is_enum()
        || t.kind == TypeKind::Struct
        || matches!(t.kind, TypeKind::TaggedEnum(_))
        || (!t.fields.is_empty() && t.name != "Option" && t.name != "Vec");
    if named {
        if out.iter().any(|o| o.name == t.name) {
            return;
        }
        out.push(t.clone());
    }
    for inner in &t.templates {
        collect_named_types(inner, out);
    }
    for field in &t.fields {
        collect_named_types(&field.field_type, out);
    }
    if let TypeKind::TaggedEnum(variants) = &t.kind {
        for (_, fields) in variants {
            for field in fields {
                collect_named_types(&field.field_type, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::rust_code::types::RustField;

    fn field(name: &str, t: RustType) -> RustField {
        RustField { name: name.to_string(), field_type: t }
    }

    #[test]
    fn schema_follows_the_wire_format() {
        let status = RustType::primitive("Status").with_kind(TypeKind::Enum(vec!["todo".into(), "done".into()]));
        let mut task = RustType::primitive("Task").with_kind(TypeKind::Struct);
        task.add_field(field("due_on", RustType::generic("Option", vec![RustType::primitive("NaiveDate")], vec![])));
        task.add_field(field("status", status));
        task.add_field(field("tags", RustType::generic("Vec", vec![RustType::primitive("String")], vec![])));

        let schema = json_schema(&task, true);
        assert_eq!(schema["required"], json!(["status", "tags"]));
        assert_eq!(schema["properties"]["dueOn"]["anyOf"][0]["format"], "date");
        assert_eq!(schema["properties"]["status"]["enum"], json!(["todo", "done"]));
        assert_eq!(schema["properties"]["tags"]["items"]["type"], "string");

        let ts = ts_declaration(&task, true);
        assert!(ts.contains("  dueOn?: string | null;"), "{ts}");
        assert!(ts.contains("  status: Status;"), "{ts}");
        assert!(ts.contains("  tags: string[];"), "{ts}");
    }
}
