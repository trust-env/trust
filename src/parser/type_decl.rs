use crate::{
    lexer::{Keyword, Token},
    parser::{
        Ast, Ast::TypeDecl, AllowedType, Parser, StructDeclaration, StructField,
        TypeAlias, TypeDeclaration, TypeDeclaration::Struct,
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
            Some(Token::Keyword(Keyword::Regex)) => match self.next_token() {
                Some(Token::OpenAngle) => match self.next_token() {
                    Some(Token::Identifier(regex_pattern)) => match self.next_token() {
                        Some(Token::CloseAngle) => Ok(AllowedType::Regex(regex_pattern)),
                        _ => {
                            self.revert(3);
                            Err("Expected '>' after regex pattern".to_string())
                        }
                    },
                    _ => {
                        self.revert(2);
                        Err("Expected regex pattern after '<'".to_string())
                    }
                },
                _ => {
                    self.revert(1);
                    Err("Expected '<' after 'Regex'".to_string())
                }
            },
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
        assert_eq!(parser_for("Decimal").try_type().unwrap(), AllowedType::Decimal);
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
