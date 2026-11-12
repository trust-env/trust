//! `map A -> B { ... }`: the crate's `trust_maps.rs` — `B::from_a(&a)`,
//! `a.to_b()` and `From<&A> for B`.

use std::path::Path;

use heck::ToSnakeCase;

use crate::{
    generator::{
        CodeGenContext,
        rust_code::{file::psql_storage_file::literal_expr, types::RustType},
    },
    parser::{MapDeclaration, MapValue},
};

/// The Rust expression reading `path` from `from` (of type `source`), and
/// whether it is an `Option`; the value is owned (cloned).
fn path_expr(context: &CodeGenContext, map: &MapDeclaration, source: &RustType, path: &[String]) -> Result<(String, RustType), String> {
    let mut expr = "from".to_string();
    let mut current = source.clone();
    let mut optional = false;
    let mut owned = false;
    for (i, part) in path.iter().enumerate() {
        let shown = path[..=i].join(".");
        let container = if i == 0 {
            current.clone()
        } else {
            context.find_type(&current.unwrap_option().name).cloned().filter(|t| !t.fields.is_empty()).ok_or_else(|| {
                format!("map {} -> {}: '{}' isn't a struct, so '{}' can't be read", map.from, map.to, path[..i].join("."), shown)
            })?
        };
        let field = container
            .fields
            .iter()
            .find(|f| &f.name == part)
            .ok_or_else(|| format!("map {} -> {}: '{}' has no field '{}'", map.from, map.to, container.name, part))?;
        let field_optional = field.field_type.is_option();
        if optional {
            expr = if field_optional {
                format!("{}.as_ref().and_then(|v| v.{}.clone())", expr, part)
            } else {
                format!("{}.as_ref().map(|v| v.{}.clone())", expr, part)
            };
            owned = true;
        } else {
            expr = format!("{}.{}", expr, part);
        }
        optional = optional || field_optional;
        current = field.field_type.clone();
    }
    if !owned {
        expr.push_str(".clone()");
    }
    let resolved = if optional && !current.is_option() {
        RustType { name: "Option".to_string(), templates: vec![current.clone()], ..current.clone() }
    } else {
        current
    };
    Ok((expr, resolved))
}

/// Writes `src/trust_maps.rs` when the crate declares `map`s; returns
/// whether it did.
pub(super) fn write_maps_module(context: &mut CodeGenContext, src_path: &Path) -> Result<bool, String> {
    if context.maps.is_empty() {
        return Ok(false);
    }
    let psql_entities = context.psql_entity_names();
    let mut out = format!(
        "//! `map A -> B {{}}` conversions.\n\nuse crate::*;\n\n/// How many `map`s the crate declares (the conversions are impls).\n#[doc(hidden)]\npub const TRUST_MAPS: usize = {};\n",
        context.maps.len()
    );
    for map in context.maps.clone() {
        let source = context
            .find_type(&map.from)
            .cloned()
            .ok_or_else(|| format!("map {} -> {}: unknown type '{}'", map.from, map.to, map.from))?;
        let target = context
            .find_type(&map.to)
            .cloned()
            .ok_or_else(|| format!("map {} -> {}: unknown type '{}'", map.from, map.to, map.to))?;
        let mut inits = Vec::new();
        for (field, value) in &map.fields {
            let target_field = target
                .fields
                .iter()
                .find(|f| &f.name == field)
                .ok_or_else(|| format!("map {} -> {}: '{}' has no field '{}'", map.from, map.to, map.to, field))?;
            let wanted = &target_field.field_type;
            let expr = match value {
                MapValue::Literal(v) => literal_expr(&map.to, field, wanted, v)?,
                MapValue::Path(path) => {
                    let (expr, got) = path_expr(context, &map, &source, path)?;
                    if got.unwrap_option().to_call_string(Vec::new()) != wanted.unwrap_option().to_call_string(Vec::new()) {
                        return Err(format!(
                            "map {} -> {}: '{}' is {} but '{}' is {}",
                            map.from,
                            map.to,
                            path.join("."),
                            got.unwrap_option().to_call_string(Vec::new()),
                            field,
                            wanted.unwrap_option().to_call_string(Vec::new())
                        ));
                    }
                    match (got.is_option(), wanted.is_option()) {
                        (false, true) => format!("Some({})", expr),
                        (true, false) => {
                            return Err(format!(
                                "map {} -> {}: '{}' is optional but '{}' isn't",
                                map.from,
                                map.to,
                                path.join("."),
                                field
                            ));
                        }
                        _ => expr,
                    }
                }
            };
            inits.push(format!("            {}: {},\n", field, expr));
        }
        let given: Vec<String> = map.fields.iter().map(|(f, _)| format!("{:?}", f)).collect();
        let complete = target.fields.iter().all(|f| map.fields.iter().any(|(m, _)| m == &f.name));
        let defaults = if psql_entities.contains(&map.to) {
            format!(
                "        crate::apply_{}_defaults(&mut to, &[{}]);\n",
                map.to.to_snake_case(),
                given.join(", ")
            )
        } else {
            String::new()
        };
        let from_snake = map.from.to_snake_case();
        let to_snake = map.to.to_snake_case();
        out.push_str(&format!(
            r#"
impl From<&{from}> for {to} {{
    /// `map {from} -> {to}`; unlisted fields take their default.
    #[allow(clippy::clone_on_copy, clippy::needless_update, unused_mut)]
    fn from(from: &{from}) -> Self {{
        let mut to = {to} {{
{inits}{rest}        }};
{defaults}        to
    }}
}}

impl {to} {{
    pub fn from_{from_snake}(from: &{from}) -> Self {{
        Self::from(from)
    }}
}}

impl {from} {{
    pub fn to_{to_snake}(&self) -> {to} {{
        {to}::from(self)
    }}
}}
"#,
            from = map.from,
            to = map.to,
            inits = inits.concat(),
            rest = if complete { "" } else { "            ..Default::default()\n" },
        ));
    }
    std::fs::write(src_path.join("trust_maps.rs"), out).map_err(|e| format!("Failed to write the maps module: {}", e))?;
    Ok(true)
}
