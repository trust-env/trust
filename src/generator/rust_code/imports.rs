use crate::generator::rust_code::render::render;
use crate::generator::rust_code::types::{RustField, RustType};
use tera::Context;

/// `use path::item;` / `use path::{a, b};` (nothing for no items).
pub fn use_statement(path: &str, items: &[&str]) -> String {
    let mut context = Context::new();
    context.insert("path", path);
    context.insert("items", items);
    render("fragments/use.rs.tera", &context)
}

/// A nested use-tree item, `prefix::{a, b}`, to pass to `use_statement`.
pub fn use_group(prefix: &str, items: &[&str]) -> String {
    let mut context = Context::new();
    context.insert("prefix", prefix);
    context.insert("items", items);
    render("fragments/use_group.rs.tera", &context)
}

pub fn common_use(items: &[&str]) -> String {
    use_statement("common", items)
}

pub fn crate_use(items: &[String]) -> String {
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    use_statement("crate", &items)
}

/// Tracks the imports a generated file needs for types that aren't
/// resolved from the `common` crate: `Uuid` (from the `uuid` crate),
/// `Decimal` (from the `rust_decimal` crate), `DateTime<Utc>` (from
/// `chrono`), and any other locally-generated domain type (`crate::{...}`).
/// `Option<T>` is transparent: only `T` needs importing.
#[derive(Default)]
pub struct LocalImports {
    pub needs_uuid: bool,
    pub needs_decimal: bool,
    pub needs_chrono: bool,
    pub types: Vec<String>,
}

/// The Rust type name `DateTime` fields resolve to.
pub const DATE_TIME: &str = "DateTime<Utc>";

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
            "Option" => {
                for inner in &rust_type.templates {
                    self.add(inner, exclude);
                }
            }
            "Uuid" => self.needs_uuid = true,
            "Decimal" => self.needs_decimal = true,
            DATE_TIME => self.needs_chrono = true,
            "String" | "i64" | "f64" | "bool" => {}
            name if name != exclude && !self.types.contains(&name.to_string()) => {
                self.types.push(name.to_string());
            }
            _ => {}
        }
    }

    /// Same as `add`, for a type already rendered to a string (e.g. the
    /// `(name, type)` field docs route files work from).
    pub fn add_type_string(&mut self, ty: &str) {
        let inner = ty
            .strip_prefix("Option<")
            .and_then(|t| t.strip_suffix('>'))
            .unwrap_or(ty);
        match inner {
            "Uuid" => self.needs_uuid = true,
            "Decimal" => self.needs_decimal = true,
            DATE_TIME => self.needs_chrono = true,
            "String" | "i64" | "f64" | "bool" => {}
            other if !self.types.iter().any(|t| t == other) => self.types.push(other.to_string()),
            _ => {}
        }
    }

    /// The `uuid`/`rust_decimal`/`chrono` imports alone (no `crate::`).
    pub fn external_use_statements(&self) -> String {
        let mut code = String::new();
        if self.needs_uuid {
            code.push_str(&use_statement("uuid", &["Uuid"]));
        }
        if self.needs_decimal {
            code.push_str(&use_statement("rust_decimal", &["Decimal"]));
        }
        if self.needs_chrono {
            code.push_str(&use_statement("chrono", &["DateTime", "Utc"]));
        }
        code
    }

    pub fn to_use_statements(&self) -> String {
        let mut code = String::new();

        code.push_str(&self.external_use_statements());
        code.push_str(&crate_use(&self.types));
        if !code.is_empty() {
            code.push('\n');
        }
        code
    }
}
