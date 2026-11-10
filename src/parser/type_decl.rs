use crate::{
    lexer::{Keyword, Token},
    parser::{
        AllowedType, Ast, Ast::TypeDecl, EnumDeclaration, Parser, StructDeclaration, StructField, TypeAlias,
        TypeDeclaration, TypeDeclaration::Struct,
    },
};

impl Parser {
    pub(super) fn try_type(&mut self) -> Result<AllowedType, String> {
        match self.next_token() {
            Some(Token::Keyword(Keyword::String)) => Ok(AllowedType::String),
            Some(Token::Keyword(Keyword::Uuid)) => Ok(AllowedType::Uuid),
            Some(Token::Keyword(Keyword::Int)) => Ok(AllowedType::Int),
            Some(Token::Keyword(Keyword::Float)) => Ok(AllowedType::Float),
            Some(Token::Keyword(Keyword::Decimal)) => Ok(AllowedType::Decimal),
            Some(Token::Keyword(Keyword::Bool)) => Ok(AllowedType::Bool),
            Some(Token::Keyword(Keyword::DateTime)) => Ok(AllowedType::DateTime),
            Some(Token::Keyword(Keyword::Option)) => {
                let pos = self.current - 1;
                if self.next_token() != Some(Token::OpenAngle) {
                    self.reset(pos);
                    return Err("Expected '<' after 'Option'".to_string());
                }
                let inner = match self.try_type() {
                    Ok(inner) => inner,
                    Err(e) => {
                        self.reset(pos);
                        return Err(e);
                    }
                };
                if matches!(inner, AllowedType::Option(_)) {
                    self.reset(pos);
                    return Err("'Option<Option<...>>' is not supported".to_string());
                }
                if self.next_token() != Some(Token::CloseAngle) {
                    self.reset(pos);
                    return Err("Expected '>' to close 'Option<...>'".to_string());
                }
                Ok(AllowedType::Option(Box::new(inner)))
            }
            Some(Token::Keyword(Keyword::Regex)) => {
                let pos = self.current - 1;
                if self.next_token() != Some(Token::OpenAngle) {
                    self.reset(pos);
                    return Err("Expected '<' after 'Regex'".to_string());
                }
                let pattern = match self.next_token() {
                    // `Regex<"^[a-z]+$">`: a real pattern.
                    Some(Token::StringLiteral(pattern)) => pattern,
                    // `Regex<slug>`: a named preset, resolved at codegen.
                    Some(Token::Identifier(preset)) => preset,
                    _ => {
                        self.reset(pos);
                        return Err("Expected a regex pattern (\"...\") or preset name after '<'".to_string());
                    }
                };
                if self.next_token() != Some(Token::CloseAngle) {
                    self.reset(pos);
                    return Err("Expected '>' after regex pattern".to_string());
                }
                Ok(AllowedType::Regex(pattern))
            }
            // `List<T>`: contextual, so a type named `List` can't be declared.
            Some(Token::Identifier(name)) if name == "List" => {
                let pos = self.current - 1;
                if self.next_token() != Some(Token::OpenAngle) {
                    self.reset(pos);
                    return Err("Expected '<' after 'List'".to_string());
                }
                let inner = match self.try_type() {
                    Ok(inner) => inner,
                    Err(e) => {
                        self.reset(pos);
                        return Err(e);
                    }
                };
                if self.next_token() != Some(Token::CloseAngle) {
                    self.reset(pos);
                    return Err("Expected '>' to close 'List<...>'".to_string());
                }
                Ok(AllowedType::List(Box::new(inner)))
            }
            Some(Token::Identifier(name)) if name == "Json" => Ok(AllowedType::Json),
            Some(Token::Identifier(name)) if name == "Date" => Ok(AllowedType::Date),
            Some(Token::Identifier(name)) if name == "Time" => Ok(AllowedType::Time),
            Some(Token::Identifier(type_name)) => Ok(AllowedType::DefinedType(type_name)),
            _ => {
                self.revert(1);
                Err("Failed to parse type".to_string())
            }
        }
    }

    fn try_type_decl_body(&mut self) -> Result<(Vec<StructField>, Option<String>), String> {
        let mut fields = Vec::new();
        let mut export = None;
        let pos = self.current;

        loop {
            match (self.next_token(), self.next_token()) {
                (Some(Token::Identifier(field_name)), Some(Token::Colon)) => {
                    let t = self.try_type()?;
                    fields.push(StructField {
                        name: field_name,
                        field_type: t,
                    });
                }
                (Some(Token::Keyword(Keyword::Export)), Some(Token::Identifier(export_name))) => {
                    export = Some(export_name);
                }
                // A field name position is unambiguous, so words that are
                // keywords elsewhere (`status`, `body`, `input`, ...) are
                // valid field names too.
                (Some(Token::Keyword(keyword)), Some(Token::Colon)) => {
                    let t = self.try_type()?;
                    fields.push(StructField {
                        name: keyword.to_string(),
                        field_type: t,
                    });
                }
                (Some(Token::CloseBrace), _) => {
                    self.revert(1);
                    break;
                }
                (Some(Token::Comma), _) => {
                    self.revert(1);
                    continue;
                }
                _ => {
                    self.reset(pos);
                    return Err("Failed to parse StructField".to_string());
                }
            }
        }
        Ok((fields, export))
    }

    /// `enum Name { a b c }` (commas optional) — `enum` is contextual,
    /// not a reserved keyword. Variants are identifiers or string literals
    /// (for wire values that aren't valid identifiers, e.g. `"in-progress"`).
    pub(super) fn try_enum_declaration(&mut self) -> Result<Ast, String> {
        let pos = self.current;
        match (self.next_token(), self.next_token(), self.next_token()) {
            (Some(Token::Identifier(kw)), Some(Token::Identifier(name)), Some(Token::OpenBrace))
                if kw == "enum" =>
            {
                let mut variants: Vec<String> = Vec::new();
                let mut export = None;
                loop {
                    match self.next_token() {
                        Some(Token::CloseBrace) => break,
                        Some(Token::Comma) => continue,
                        Some(Token::Keyword(Keyword::Export)) => match self.next_token() {
                            Some(Token::Identifier(export_name)) => export = Some(export_name),
                            _ => {
                                self.reset(pos);
                                return Err("Expected a name after 'export'".to_string());
                            }
                        },
                        Some(Token::Identifier(v)) | Some(Token::StringLiteral(v)) => {
                            if variants.contains(&v) {
                                self.reset(pos);
                                return Err(format!("enum '{}' declares '{}' twice", name, v));
                            }
                            variants.push(v)
                        }
                        Some(Token::Keyword(k)) => variants.push(k.to_string()),
                        _ => {
                            self.reset(pos);
                            return Err(format!("Failed to parse enum '{}'", name));
                        }
                    }
                }
                if variants.is_empty() {
                    self.reset(pos);
                    return Err(format!("enum '{}' must declare at least one variant", name));
                }
                Ok(TypeDecl(TypeDeclaration::Enum(EnumDeclaration { name, variants, export })))
            }
            _ => {
                self.reset(pos);
                Err("Failed to parse enum declaration".to_string())
            }
        }
    }

    pub(super) fn try_type_declaration(&mut self) -> Result<Ast, String> {
        match (self.next_token(), self.next_token(), self.next_token()) {
            (
                Some(Token::Keyword(Keyword::Type)),
                Some(Token::Identifier(name)),
                Some(Token::Equal),
            ) => Ok(TypeDecl(TypeDeclaration::TypeAlias(TypeAlias {
                name: name.clone(),
                aliased_type: self.try_type()?,
            }))),
            (
                Some(Token::Keyword(Keyword::Type)),
                Some(Token::Identifier(name)),
                Some(Token::OpenBrace),
            ) => {
                let (fields, export) = self.try_type_decl_body()?;
                Ok(TypeDecl(Struct(StructDeclaration {
                    name: name.clone(),
                    fields,
                    export,
                })))
            }
            _ => {
                self.revert(3);
                Err("Failed to parse TypeDeclaration".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parser_for(src: &str) -> Parser {
        let tokens = Lexer::new(&src.to_string()).run().expect("lex should succeed");
        Parser::new(tokens)
    }

    #[test]
    fn try_type_resolves_every_allowed_type_variant() {
        assert_eq!(parser_for("String").try_type().unwrap(), AllowedType::String);
        assert_eq!(parser_for("Uuid").try_type().unwrap(), AllowedType::Uuid);
        assert_eq!(parser_for("Int").try_type().unwrap(), AllowedType::Int);
        assert_eq!(parser_for("Float").try_type().unwrap(), AllowedType::Float);
        assert_eq!(
            parser_for("Decimal").try_type().unwrap(),
            AllowedType::Decimal
        );
        assert_eq!(parser_for("Bool").try_type().unwrap(), AllowedType::Bool);
        assert_eq!(
            parser_for("DateTime").try_type().unwrap(),
            AllowedType::DateTime
        );
        assert_eq!(
            parser_for("Option<String>").try_type().unwrap(),
            AllowedType::Option(Box::new(AllowedType::String))
        );
        assert_eq!(
            parser_for("Option<Sticker>").try_type().unwrap(),
            AllowedType::Option(Box::new(AllowedType::DefinedType("Sticker".to_string())))
        );
        assert_eq!(
            parser_for("Regex<slug>").try_type().unwrap(),
            AllowedType::Regex("slug".to_string())
        );
        assert_eq!(
            parser_for("SomeCustomType").try_type().unwrap(),
            AllowedType::DefinedType("SomeCustomType".to_string())
        );
    }

    #[test]
    fn keywords_are_allowed_as_field_names() {
        let ast = parser_for("type Order { id: Int, status: String, input: Int, body: String }")
            .try_type_declaration()
            .unwrap();
        match ast {
            Ast::TypeDecl(TypeDeclaration::Struct(s)) => {
                let names: Vec<&str> = s.fields.iter().map(|f| f.name.as_str()).collect();
                assert_eq!(names, vec!["id", "status", "input", "body"]);
            }
            other => panic!("expected a Struct, got {:?}", other),
        }
    }

    #[test]
    fn malformed_option_types_are_rejected() {
        for src in [
            "Option String",
            "Option<String",
            "Option<>",
            "Option<Option<Int>>",
        ] {
            let mut parser = parser_for(src);
            assert!(parser.try_type().is_err(), "'{}' should not parse", src);
            assert_eq!(
                parser.current, 0,
                "'{}' must not consume tokens on failure",
                src
            );
        }
    }

    #[test]
    fn struct_fields_accept_the_new_types() {
        let ast = parser_for(
            "type Order { id: Int, paid: Bool, created_at: DateTime, note: Option<String> }",
        )
        .try_type_declaration()
        .unwrap();
        match ast {
            Ast::TypeDecl(TypeDeclaration::Struct(s)) => {
                assert_eq!(s.fields[1].field_type, AllowedType::Bool);
                assert_eq!(s.fields[2].field_type, AllowedType::DateTime);
                assert_eq!(
                    s.fields[3].field_type,
                    AllowedType::Option(Box::new(AllowedType::String))
                );
            }
            other => panic!("expected a Struct, got {:?}", other),
        }
    }

    #[test]
    fn type_alias_form_parses() {
        let ast = parser_for("type UserId = Int").try_type_declaration().unwrap();
        match ast {
            Ast::TypeDecl(TypeDeclaration::TypeAlias(alias)) => {
                assert_eq!(alias.name, "UserId");
                assert_eq!(alias.aliased_type, AllowedType::Int);
            }
            other => panic!("expected a TypeAlias, got {:?}", other),
        }
    }

    #[test]
    fn struct_form_parses_fields_and_export() {
        let ast = parser_for(
            "type User {
                id: Int,
                name: String
                export ExportedUser
            }",
        )
        .try_type_declaration()
        .unwrap();
        match ast {
            Ast::TypeDecl(TypeDeclaration::Struct(s)) => {
                assert_eq!(s.name, "User");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].name, "id");
                assert_eq!(s.fields[0].field_type, AllowedType::Int);
                assert_eq!(s.fields[1].name, "name");
                assert_eq!(s.fields[1].field_type, AllowedType::String);
                assert_eq!(s.export, Some("ExportedUser".to_string()));
            }
            other => panic!("expected a Struct, got {:?}", other),
        }
    }

    #[test]
    fn struct_fields_without_trailing_comma_still_parse() {
        // No comma between "id: Int" and "name: String" — separators are
        // effectively optional (Comma is just skipped, not required).
        let ast = parser_for("type User { id: Int name: String }")
            .try_type_declaration()
            .unwrap();
        match ast {
            Ast::TypeDecl(TypeDeclaration::Struct(s)) => {
                assert_eq!(s.fields.len(), 2);
            }
            other => panic!("expected a Struct, got {:?}", other),
        }
    }

    #[test]
    fn struct_with_no_export_has_none() {
        let ast = parser_for("type User { id: Int }").try_type_declaration().unwrap();
        match ast {
            Ast::TypeDecl(TypeDeclaration::Struct(s)) => assert_eq!(s.export, None),
            other => panic!("expected a Struct, got {:?}", other),
        }
    }
}
