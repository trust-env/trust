use crate::{
    generator::{
        CodeGen, CodeGenContext,
        rust_code::types::{RustField, RustType},
    },
    parser::{AllowedType, StructDeclaration, TypeDeclaration},
};

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
            AllowedType::Regex(_) => Ok(RustType::primitive("String")),
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
        let mut t = RustType::primitive(&s.name);

        for field in &s.fields {
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
        }
    }
}
