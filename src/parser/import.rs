use std::path::PathBuf;

use crate::{
    lexer::{Keyword, Token},
    parser::{Ast, ImportDeclaration, ImportItem, ModuleKind, Parser},
};

impl Parser {
    fn try_import_items(&mut self) -> Result<ImportItem, String> {
        let mut revert_count = 1;

        match self.next_token() {
            Some(Token::OpenBrace) => {
                let mut items = Vec::new();

                loop {
                    revert_count += 1;
                    match self.next_token() {
                        Some(Token::Identifier(item)) => items.push(item),
                        Some(Token::Comma) => continue,
                        Some(Token::CloseBrace) => break,
                        _ => {
                            self.revert(revert_count);
                            return Err("Expected identifier or '}' in import items".to_string());
                        }
                    }
                }
                Ok(ImportItem::Specific(items))
            }
            Some(Token::Identifier(item)) => Ok(ImportItem::All(item)),
            _ => {
                self.revert(revert_count);
                Err("Expected '{' or identifier in import items".to_string())
            }
        }
    }

    fn try_import_module(&mut self) -> Result<ModuleKind, String> {
        if self.next_token() != Some(Token::Keyword(Keyword::From)) {
            self.revert(1);
            return Err("Expected 'from' keyword in import declaration".to_string());
        }
        match self.next_token() {
            Some(Token::StringLiteral(module_path)) => {
                if module_path.starts_with("git@")
                    || module_path.starts_with("http://")
                    || module_path.starts_with("https://")
                    || module_path.ends_with(".git")
                {
                    Ok(ModuleKind::Git(module_path))
                } else {
                    Ok(ModuleKind::Path(PathBuf::from(module_path)))
                }
            }
            _ => {
                self.revert(2);
                Err("Expected quoted module path after 'from' keyword in import declaration".to_string())
            }
        }
    }

    pub(super) fn try_import_declaration(&mut self) -> Result<Ast, String> {
        if self.next_token() != Some(Token::Keyword(Keyword::Import)) {
            self.revert(1);
            return Err("Expected 'import' keyword".to_string());
        }

        let items = self.try_import_items()?;
        let module = self.try_import_module()?;

        Ok(Ast::Import(ImportDeclaration { items, module }))
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
    fn bare_identifier_import_is_item_all() {
        let ast = parser_for(r#"import SQLConnector from "./lib/shared.tr""#)
            .try_import_declaration()
            .unwrap();
        match ast {
            Ast::Import(decl) => {
                match decl.items {
                    ImportItem::All(name) => assert_eq!(name, "SQLConnector"),
                    other => panic!("expected ImportItem::All, got {:?}", other),
                }
                match decl.module {
                    ModuleKind::Path(path) => assert_eq!(path, PathBuf::from("./lib/shared.tr")),
                    other => panic!("expected ModuleKind::Path, got {:?}", other),
                }
            }
            other => panic!("expected Ast::Import, got {:?}", other),
        }
    }

    #[test]
    fn braced_list_import_is_item_specific() {
        let ast = parser_for(r#"import { A, B, C } from "./mod/x.tr""#)
            .try_import_declaration()
            .unwrap();
        match ast {
            Ast::Import(decl) => match decl.items {
                ImportItem::Specific(names) => {
                    assert_eq!(names, vec!["A".to_string(), "B".to_string(), "C".to_string()]);
                }
                other => panic!("expected ImportItem::Specific, got {:?}", other),
            },
            other => panic!("expected Ast::Import, got {:?}", other),
        }
    }

    #[test]
    fn module_path_classification_rules() {
        let cases = [
            (r#"import X from "git@github.com:org/repo.git""#, true),
            (r#"import X from "http://example.com/mod.tr""#, true),
            (r#"import X from "https://example.com/mod.tr""#, true),
            (r#"import X from "ssh://git@github.com/org/repo.git""#, true),
            (r#"import X from "./local/path.tr""#, false),
        ];
        for (src, expect_git) in cases {
            let ast = parser_for(src).try_import_declaration().unwrap();
            match ast {
                Ast::Import(decl) => {
                    let is_git = matches!(decl.module, ModuleKind::Git(_));
                    assert_eq!(is_git, expect_git, "wrong classification for {:?}", src);
                }
                other => panic!("expected Ast::Import, got {:?}", other),
            }
        }
    }

    #[test]
    fn missing_from_keyword_is_a_clear_error() {
        let err = parser_for(r#"import X "./mod/x.tr""#).try_import_declaration().unwrap_err();
        assert_eq!(err, "Expected 'from' keyword in import declaration");
    }

    #[test]
    fn missing_quoted_path_is_a_clear_error() {
        let err = parser_for("import X from NotAString").try_import_declaration().unwrap_err();
        assert_eq!(
            err,
            "Expected quoted module path after 'from' keyword in import declaration"
        );
    }

    #[test]
    fn missing_import_keyword_is_a_clear_error() {
        let err = parser_for(r#"from "./mod/x.tr""#).try_import_declaration().unwrap_err();
        assert_eq!(err, "Expected 'import' keyword".to_string());
    }
}
