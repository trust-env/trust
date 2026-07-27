use crate::generator::rust_code::types::{RustField, RustType};

pub fn use_statement(path: &str, items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [single] => format!("use {}::{};\n", path, single),
        _ => format!("use {}::{{{}}};\n", path, items.join(", ")),
    }
}

pub fn common_use(items: &[&str]) -> String {
    use_statement("common", items)
}

pub fn crate_use(items: &[String]) -> String {
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    use_statement("crate", &items)
}

/// Tracks the imports a generated file needs for types that aren't
/// resolved from the `common` crate: `Uuid` (from the `uuid` crate)
/// and any other locally-generated domain type (`crate::{...}`).
#[derive(Default)]
pub struct LocalImports {
    pub needs_uuid: bool,
    pub types: Vec<String>,
}

impl LocalImports {
    pub fn from_fields(fields: &[RustField], exclude: &str) -> Self {
        let mut imports = LocalImports::default();

        for field in fields {
            imports.add(&field.field_type, exclude);
        }
        imports
    }

    pub fn add(&mut self, rust_type: &RustType, exclude: &str) {
        match rust_type.name.as_str() {
            "Uuid" => self.needs_uuid = true,
            "String" | "i64" => {}
            name if name != exclude && !self.types.contains(&name.to_string()) => {
                self.types.push(name.to_string());
            }
            _ => {}
        }
    }

    pub fn to_use_statements(&self) -> String {
        let mut code = String::new();

        if self.needs_uuid {
            code.push_str("use uuid::Uuid;\n");
        }
        code.push_str(&crate_use(&self.types));
        if !code.is_empty() {
            code.push('\n');
        }
        code
    }
}
