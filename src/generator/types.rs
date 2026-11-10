use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::types::{RustField, RustType, TypeKind},
    },
    parser::{AllowedType, EnumDeclaration, StructDeclaration, TypeDeclaration},
};
use heck::ToUpperCamelCase;

/// `Regex<preset>` names and the pattern each stands for.
pub const REGEX_PRESETS: &[(&str, &str)] = &[
    ("slug", r"^[a-z0-9]+(?:-[a-z0-9]+)*$"),
    ("email", r"^[^@\s]+@[^@\s]+\.[^@\s]+$"),
    ("alpha", r"^[A-Za-z]+$"),
    ("alphanumeric", r"^[A-Za-z0-9]+$"),
    ("numeric", r"^[0-9]+$"),
    ("hex", r"^[0-9a-fA-F]+$"),
    ("identifier", r"^[A-Za-z_][A-Za-z0-9_]*$"),
];

/// Resolves a `Regex<...>` argument: a preset name, or a literal pattern
/// (checked for syntax here so a typo fails at compile time).
pub fn resolve_regex(pattern: &str) -> Result<String, String> {
    if let Some((_, p)) = REGEX_PRESETS.iter().find(|(name, _)| *name == pattern) {
        return Ok(p.to_string());
    }
    if pattern.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(format!(
            "Unknown Regex preset '{}' (presets: {}); write a pattern as Regex<\"...\">",
            pattern,
            REGEX_PRESETS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
        ));
    }
    regex::Regex::new(pattern).map_err(|e| format!("Invalid Regex<\"{}\">: {}", pattern, e))?;
    Ok(pattern.to_string())
}

/// The Rust variant name of an enum wire value (`in-progress` -> `InProgress`).
pub fn enum_variant_ident(wire: &str) -> String {
    let ident = wire.to_upper_camel_case();
    if ident.is_empty() || ident.starts_with(|c: char| c.is_ascii_digit()) {
        format!("V{}", ident)
    } else {
        ident
    }
}

/// The Rust type `Json` fields resolve to.
pub const JSON_VALUE: &str = "serde_json::Value";

impl CodeGen {
    fn resolve_type_reference(
        &self,
        context: &mut CodeGenContext,
        type_ref: &crate::parser::AllowedType,
    ) -> Result<RustType, String> {
        match type_ref {
            AllowedType::String => Ok(RustType::primitive("String")),
            AllowedType::Uuid => Ok(RustType::primitive("Uuid")),
            AllowedType::Int => Ok(RustType::primitive("i64")),
            AllowedType::Float => Ok(RustType::primitive("f64")),
            AllowedType::Decimal => Ok(RustType::primitive("Decimal")),
            AllowedType::Bool => Ok(RustType::primitive("bool")),
            AllowedType::DateTime => Ok(RustType::primitive("DateTime<Utc>")),
            AllowedType::Option(inner) => {
                let inner = self.resolve_type_reference(context, inner)?;
                Ok(RustType::generic("Option", vec![inner], vec![]))
            }
            AllowedType::List(inner) => {
                let inner = self.resolve_type_reference(context, inner)?;
                Ok(RustType::generic("Vec", vec![inner], vec![]))
            }
            AllowedType::Json => Ok(RustType::primitive(JSON_VALUE)),
            AllowedType::Date => Ok(RustType::primitive("NaiveDate")),
            AllowedType::Time => Ok(RustType::primitive("NaiveTime")),
            AllowedType::Regex(pattern) => {
                Ok(RustType::primitive("String").with_kind(TypeKind::Regex(resolve_regex(pattern)?)))
            }
            AllowedType::DefinedType(name) => {
                if let Some(t) = context.find_type(name) {
                    Ok(t.clone())
                } else {
                    Err(format!("Undefined type reference: {}", name))
                }
            }
        }
    }

    fn resolve_struct_declaration(
        &self,
        context: &mut CodeGenContext,
        s: &StructDeclaration,
    ) -> Result<RustType, String> {
        let mut t = RustType::primitive(&s.name).with_kind(TypeKind::Struct);

        for (i, field) in s.fields.iter().enumerate() {
            if s.fields[..i].iter().any(|f| f.name == field.name) {
                return Err(format!("type '{}' declares field '{}' twice", s.name, field.name));
            }
            let field_type = self.resolve_type_reference(context, &field.field_type)?;

            t.add_field(RustField {
                name: field.name.clone(),
                field_type,
            });
        }
        Ok(t)
    }

    fn resolve_type_alias_declaration(
        &self,
        context: &mut CodeGenContext,
        a: &crate::parser::TypeAlias,
    ) -> Result<RustType, String> {
        let aliased_type = self.resolve_type_reference(context, &a.aliased_type)?;
        let mut t = RustType::primitive(&a.name);

        t.add_field(RustField {
            name: a.name.clone(),
            field_type: aliased_type,
        });
        Ok(t)
    }

    pub(super) fn resolve_type_declaration(
        &self,
        context: &mut CodeGenContext,
        t: &TypeDeclaration,
    ) -> Result<RustType, String> {
        match t {
            TypeDeclaration::Struct(s) => self.resolve_struct_declaration(context, s),
            TypeDeclaration::TypeAlias(a) => self.resolve_type_alias_declaration(context, a),
            TypeDeclaration::Enum(e) => Ok(Self::resolve_enum_declaration(e)),
        }
    }

    fn resolve_enum_declaration(e: &EnumDeclaration) -> RustType {
        RustType::primitive(&e.name).with_kind(TypeKind::Enum(e.variants.clone()))
    }

    /// Resolves every declaration in `decls` regardless of order: a type
    /// may reference one declared later in the file. Repeats until nothing
    /// new resolves; whatever is left is a genuinely unknown reference.
    pub(super) fn resolve_type_declarations(
        &self,
        context: &mut CodeGenContext,
        decls: &[&TypeDeclaration],
    ) -> Result<(), String> {
        let mut pending: Vec<&TypeDeclaration> = decls.to_vec();
        loop {
            let mut next = Vec::new();
            let mut last_error = None;
            for decl in &pending {
                match self.resolve_type_declaration(context, decl) {
                    Ok(resolved) => context.add_type(resolved),
                    Err(e) => {
                        last_error = Some(e);
                        next.push(*decl);
                    }
                }
            }
            if next.is_empty() {
                return Ok(());
            }
            if next.len() == pending.len() {
                return Err(last_error.unwrap_or_default());
            }
            pending = next;
        }
    }
}
