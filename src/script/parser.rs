//! Recursive descent parser of a `logic { ... }` script.
//!
//! Precedence, loosest first: `or`, `and`, `not`, comparisons (`==` `!=`
//! `<` `<=` `>` `>=` `in` `not in`), `??`, `+ -`, `* / %`, unary `-`, then
//! postfix `.field` / `.method(...)`.

use super::ast::*;
use super::lexer::{Piece, Tok, Token};
use super::{Pos, ScriptError};

/// Words that can't name a variable.
pub const KEYWORDS: &[&str] = &[
    "let", "var", "if", "else", "for", "in", "return", "fail", "true", "false", "none", "and",
    "or", "not", "try",
];

type R<T> = Result<T, ScriptError>;

/// A literal's `field: value` pairs and its `..base`.
type Fields = (Vec<(String, Expr)>, Option<Box<Expr>>);

pub fn parse(tokens: Vec<Token>) -> R<Vec<Stmt>> {
    let mut parser = Parser { tokens, i: 0 };
    let mut body = Vec::new();
    while !parser.at_eof() {
        body.push(parser.stmt()?);
    }
    Ok(body)
}

/// A top-level `logic {}` block: `extern fn` declarations only.
pub fn parse_library(tokens: Vec<Token>) -> R<Vec<ExternFn>> {
    let mut parser = Parser { tokens, i: 0 };
    let mut out = Vec::new();
    while !parser.at_eof() {
        let pos = parser.pos();
        if !(parser.eat_word("extern") && parser.eat_word("fn")) {
            return parser.expected("'extern fn name(arg: Type) -> Type from \"./file.rs\"'");
        }
        let name = parser.ident("a function name")?;
        parser.expect_sym("(")?;
        let mut params = Vec::new();
        while !parser.eat_sym(")") {
            if !params.is_empty() {
                parser.expect_sym(",")?;
            }
            let param = parser.ident("a parameter name")?;
            parser.expect_sym(":")?;
            let by_ref = parser.eat_sym("&");
            params.push((param, parser.type_expr()?, by_ref));
        }
        parser.expect_sym("->")?;
        let ret = parser.type_expr()?;
        if !parser.eat_word("from") {
            return parser.expected("'from \"./file.rs\"'");
        }
        let from = match parser.advance().tok {
            Tok::Str(pieces) => match pieces.as_slice() {
                [Piece::Text(path)] => path.clone(),
                _ => {
                    return Err(ScriptError::new(
                        pos,
                        "the path can't interpolate".to_string(),
                    ));
                }
            },
            other => {
                return Err(ScriptError::new(
                    pos,
                    format!("expected a quoted path, found {}", Parser::describe(&other)),
                ));
            }
        };
        out.push(ExternFn {
            name,
            params,
            ret,
            from,
            pos,
        });
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.i.min(self.tokens.len() - 1)]
    }

    fn peek_at(&self, ahead: usize) -> &Tok {
        &self.tokens[(self.i + ahead).min(self.tokens.len() - 1)].tok
    }

    fn pos(&self) -> Pos {
        self.peek().pos
    }

    fn at_eof(&self) -> bool {
        self.peek().tok == Tok::Eof
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if token.tok != Tok::Eof {
            self.i += 1;
        }
        token
    }

    fn is_sym(&self, symbol: &str) -> bool {
        matches!(&self.peek().tok, Tok::Sym(s) if *s == symbol)
    }

    fn is_word(&self, word: &str) -> bool {
        matches!(&self.peek().tok, Tok::Ident(w) if w == word)
    }

    fn eat_sym(&mut self, symbol: &str) -> bool {
        if self.is_sym(symbol) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.is_word(word) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn describe(tok: &Tok) -> String {
        match tok {
            Tok::Ident(w) => format!("'{}'", w),
            Tok::Int(n) | Tok::Float(n) => format!("'{}'", n),
            Tok::Str(_) => "a string".to_string(),
            Tok::Sym(s) => format!("'{}'", s),
            Tok::Eof => "the end of the block".to_string(),
        }
    }

    fn expected<T>(&self, what: &str) -> R<T> {
        Err(ScriptError::new(
            self.pos(),
            format!(
                "expected {}, found {}",
                what,
                Self::describe(&self.peek().tok)
            ),
        ))
    }

    fn expect_sym(&mut self, symbol: &str) -> R<()> {
        if self.eat_sym(symbol) {
            Ok(())
        } else {
            self.expected(&format!("'{}'", symbol))
        }
    }

    fn ident(&mut self, what: &str) -> R<String> {
        match &self.peek().tok {
            Tok::Ident(w) if !KEYWORDS.contains(&w.as_str()) => {
                let w = w.clone();
                self.i += 1;
                Ok(w)
            }
            _ => self.expected(what),
        }
    }

    fn block(&mut self) -> R<Vec<Stmt>> {
        self.expect_sym("{")?;
        let mut body = Vec::new();
        while !self.eat_sym("}") {
            if self.at_eof() {
                return self.expected("'}'");
            }
            body.push(self.stmt()?);
        }
        Ok(body)
    }

    fn stmt(&mut self) -> R<Stmt> {
        let pos = self.pos();
        if self.is_word("let") || self.is_word("var") {
            let mutable = self.is_word("var");
            self.i += 1;
            let name = self.ident("a variable name")?;
            let ty = if self.eat_sym(":") {
                Some(self.type_expr()?)
            } else {
                None
            };
            self.expect_sym("=")?;
            let value = self.expr(true)?;
            return Ok(Stmt::Let {
                name,
                mutable,
                ty,
                value,
                pos,
            });
        }
        if self.eat_word("if") {
            return self.if_rest(pos);
        }
        if self.eat_word("for") {
            let var = self.ident("a loop variable")?;
            if !self.eat_word("in") {
                return self.expected("'in'");
            }
            let iter = self.expr(false)?;
            let body = self.block()?;
            return Ok(Stmt::For {
                var,
                iter,
                body,
                pos,
            });
        }
        if self.eat_word("return") {
            let value = self.expr(true)?;
            return Ok(Stmt::Return { value, pos });
        }
        if self.eat_word("fail") {
            let (variant, message) = self.fail_rest()?;
            return Ok(Stmt::Fail {
                variant,
                message,
                pos,
            });
        }
        if self.is_word("while") || self.is_word("loop") {
            return Err(ScriptError::new(
                pos,
                "there is no 'while'/'loop': iterate a list with 'for x in list { ... }' (so every script ends)".to_string(),
            ));
        }
        let expr = self.expr(true)?;
        if self.eat_sym("=") {
            let value = self.expr(true)?;
            return Ok(Stmt::Assign {
                target: expr,
                value,
                pos,
            });
        }
        Ok(Stmt::Expr { expr, pos })
    }

    fn if_rest(&mut self, pos: Pos) -> R<Stmt> {
        let cond = if self.eat_word("let") {
            if let (Tok::Ident(ty), Tok::Sym(".")) =
                (self.peek_at(0).clone(), self.peek_at(1).clone())
                && ty.starts_with(|c: char| c.is_uppercase())
            {
                return self.if_variant_rest(pos, ty);
            }
            let name = self.ident("a variable name")?;
            self.expect_sym("=")?;
            Cond::Let(name, self.expr(false)?)
        } else {
            Cond::Bool(self.expr(false)?)
        };
        let then = self.block()?;
        let otherwise = if self.eat_word("else") {
            if self.is_word("if") {
                let pos = self.pos();
                self.i += 1;
                Some(vec![self.if_rest(pos)?])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then,
            otherwise,
            pos,
        })
    }

    /// After `if let Type`: `.variant { field, field: name } = value { ... }`.
    fn if_variant_rest(&mut self, pos: Pos, ty: String) -> R<Stmt> {
        let pattern_pos = self.pos();
        self.i += 2;
        let variant = match self.advance().tok {
            Tok::Ident(v) => v,
            other => {
                return Err(ScriptError::new(
                    pattern_pos,
                    format!("expected a variant name, found {}", Self::describe(&other)),
                ));
            }
        };
        let mut bindings = Vec::new();
        if self.eat_sym("{") {
            while !self.eat_sym("}") {
                if self.eat_sym(",") {
                    continue;
                }
                let field = self.ident("a field name")?;
                let name = if self.eat_sym(":") {
                    self.ident("a variable name")?
                } else {
                    field.clone()
                };
                bindings.push((field, name));
            }
        }
        self.expect_sym("=")?;
        let value = self.expr(false)?;
        let cond = Cond::Variant {
            ty,
            variant,
            bindings,
            value,
            pos: pattern_pos,
        };
        let then = self.block()?;
        let otherwise = if self.eat_word("else") {
            if self.is_word("if") {
                let pos = self.pos();
                self.i += 1;
                Some(vec![self.if_rest(pos)?])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then,
            otherwise,
            pos,
        })
    }

    /// After `if` in a value: `c { a } else { b }` / `else if ...`.
    fn if_value(&mut self, pos: Pos) -> R<Expr> {
        let cond = self.expr(false)?;
        self.expect_sym("{")?;
        let then = self.expr(true)?;
        self.expect_sym("}")?;
        if !self.eat_word("else") {
            return self.expected("'else': an 'if' giving a value needs both branches");
        }
        let otherwise = if self.is_word("if") {
            let pos = self.advance().pos;
            self.if_value(pos)?
        } else {
            self.expect_sym("{")?;
            let value = self.expr(true)?;
            self.expect_sym("}")?;
            value
        };
        Ok(Expr {
            kind: ExprKind::If(Box::new(cond), Box::new(then), Box::new(otherwise)),
            pos,
        })
    }

    fn fail_rest(&mut self) -> R<(String, Option<Expr>)> {
        let variant = self.ident("an error name (fail NotFound, fail Conflict(\"...\"))")?;
        let message = if self.eat_sym("(") {
            let message = self.expr(true)?;
            self.expect_sym(")")?;
            Some(message)
        } else {
            None
        };
        Ok((variant, message))
    }

    fn type_expr(&mut self) -> R<TypeExpr> {
        let pos = self.pos();
        let name = self.ident("a type")?;
        if self.eat_sym("<") {
            let mut args = vec![self.type_expr()?];
            while self.eat_sym(",") {
                args.push(self.type_expr()?);
            }
            self.expect_sym(">")?;
            return Ok(TypeExpr::Generic(name, args, pos));
        }
        Ok(TypeExpr::Named(name, pos))
    }

    /// `structs`: whether `Name { ... }` is a struct literal here (not in
    /// an `if`/`for` head, where `{` opens the block).
    fn expr(&mut self, structs: bool) -> R<Expr> {
        self.or(structs)
    }

    fn or(&mut self, s: bool) -> R<Expr> {
        let mut left = self.and(s)?;
        while self.is_word("or") {
            let pos = self.advance().pos;
            let right = self.and(s)?;
            left = Expr {
                kind: ExprKind::Binary(BinOp::Or, Box::new(left), Box::new(right)),
                pos,
            };
        }
        Ok(left)
    }

    fn and(&mut self, s: bool) -> R<Expr> {
        let mut left = self.not(s)?;
        while self.is_word("and") {
            let pos = self.advance().pos;
            let right = self.not(s)?;
            left = Expr {
                kind: ExprKind::Binary(BinOp::And, Box::new(left), Box::new(right)),
                pos,
            };
        }
        Ok(left)
    }

    fn not(&mut self, s: bool) -> R<Expr> {
        if self.is_word("not") {
            let pos = self.advance().pos;
            let inner = self.not(s)?;
            return Ok(Expr {
                kind: ExprKind::Not(Box::new(inner)),
                pos,
            });
        }
        self.comparison(s)
    }

    fn comparison(&mut self, s: bool) -> R<Expr> {
        let left = self.coalesce(s)?;
        let pos = self.pos();
        let op = match &self.peek().tok {
            Tok::Sym("==") => BinOp::Eq,
            Tok::Sym("!=") => BinOp::Ne,
            Tok::Sym("<") => BinOp::Lt,
            Tok::Sym("<=") => BinOp::Le,
            Tok::Sym(">") => BinOp::Gt,
            Tok::Sym(">=") => BinOp::Ge,
            Tok::Ident(w) if w == "in" => BinOp::In,
            Tok::Ident(w)
                if w == "not" && matches!(self.peek_at(1), Tok::Ident(n) if n == "in") =>
            {
                self.i += 1;
                BinOp::NotIn
            }
            _ => return Ok(left),
        };
        self.i += 1;
        let right = self.coalesce(s)?;
        if matches!(
            &self.peek().tok,
            Tok::Sym("==" | "!=" | "<" | "<=" | ">" | ">=")
        ) {
            return Err(ScriptError::new(
                self.pos(),
                "comparisons don't chain: use 'and'".to_string(),
            ));
        }
        Ok(Expr {
            kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
            pos,
        })
    }

    fn coalesce(&mut self, s: bool) -> R<Expr> {
        let left = self.additive(s)?;
        if self.is_sym("??") {
            let pos = self.advance().pos;
            let right = if self.eat_word("fail") {
                let fail_pos = self.pos();
                let (variant, message) = self.fail_rest()?;
                Expr {
                    kind: ExprKind::Fail(variant, message.map(Box::new)),
                    pos: fail_pos,
                }
            } else {
                self.coalesce(s)?
            };
            return Ok(Expr {
                kind: ExprKind::Binary(BinOp::Coalesce, Box::new(left), Box::new(right)),
                pos,
            });
        }
        Ok(left)
    }

    fn additive(&mut self, s: bool) -> R<Expr> {
        let mut left = self.multiplicative(s)?;
        loop {
            let op = match &self.peek().tok {
                Tok::Sym("+") => BinOp::Add,
                Tok::Sym("-") => BinOp::Sub,
                _ => return Ok(left),
            };
            let pos = self.advance().pos;
            let right = self.multiplicative(s)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                pos,
            };
        }
    }

    fn multiplicative(&mut self, s: bool) -> R<Expr> {
        let mut left = self.unary(s)?;
        loop {
            let op = match &self.peek().tok {
                Tok::Sym("*") => BinOp::Mul,
                Tok::Sym("/") => BinOp::Div,
                Tok::Sym("%") => BinOp::Rem,
                _ => return Ok(left),
            };
            let pos = self.advance().pos;
            let right = self.unary(s)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                pos,
            };
        }
    }

    fn unary(&mut self, s: bool) -> R<Expr> {
        if self.is_word("try") {
            let pos = self.advance().pos;
            let inner = self.unary(s)?;
            return Ok(Expr {
                kind: ExprKind::Try(Box::new(inner)),
                pos,
            });
        }
        if self.is_sym("-") {
            let pos = self.advance().pos;
            let inner = self.unary(s)?;
            return Ok(Expr {
                kind: ExprKind::Neg(Box::new(inner)),
                pos,
            });
        }
        self.postfix(s)
    }

    fn postfix(&mut self, s: bool) -> R<Expr> {
        let mut expr = self.primary(s)?;
        while self.is_sym(".") {
            let pos = self.advance().pos;
            let name = match self.advance().tok {
                Tok::Ident(w) => w,
                other => {
                    return Err(ScriptError::new(
                        pos,
                        format!(
                            "expected a field or method name after '.', found {}",
                            Self::describe(&other)
                        ),
                    ));
                }
            };
            if self.eat_sym("(") {
                let args = self.args()?;
                expr = Expr {
                    kind: ExprKind::Method(Box::new(expr), name, args),
                    pos,
                };
            } else {
                let is_type = matches!(&expr.kind, ExprKind::Var(t) if t.starts_with(|c: char| c.is_uppercase()));
                if s && is_type && self.is_sym("{") {
                    self.i += 1;
                    let (fields, spread) = self.fields()?;
                    if spread.is_some() {
                        return Err(ScriptError::new(
                            pos,
                            "'..base' isn't available on enum values".to_string(),
                        ));
                    }
                    let ExprKind::Var(ty) = expr.kind else {
                        unreachable!()
                    };
                    expr = Expr {
                        kind: ExprKind::Variant {
                            ty,
                            variant: name,
                            fields,
                        },
                        pos,
                    };
                } else {
                    expr = Expr {
                        kind: ExprKind::Field(Box::new(expr), name),
                        pos,
                    };
                }
            }
        }
        Ok(expr)
    }

    /// After `(`: `a, name: b, x => c` up to `)`.
    fn args(&mut self) -> R<Vec<Arg>> {
        let mut args = Vec::new();
        while !self.eat_sym(")") {
            if !args.is_empty() {
                self.expect_sym(",")?;
                if self.eat_sym(")") {
                    break;
                }
            }
            let name = match (self.peek_at(0), self.peek_at(1)) {
                (Tok::Ident(n), Tok::Sym(":")) => {
                    let n = n.clone();
                    self.i += 2;
                    Some(n)
                }
                _ => None,
            };
            args.push(Arg {
                name,
                value: self.expr(true)?,
            });
        }
        Ok(args)
    }

    fn fields(&mut self) -> R<Fields> {
        let mut fields = Vec::new();
        let mut spread = None;
        // Commas between fields are optional, like everywhere in `.tr`.
        while !self.eat_sym("}") {
            if self.eat_sym(",") {
                continue;
            }
            if self.at_eof() {
                return self.expected("'}'");
            }
            if self.is_sym("..") {
                let pos = self.advance().pos;
                if spread.is_some() {
                    return Err(ScriptError::new(
                        pos,
                        "only one '..base' per literal".to_string(),
                    ));
                }
                spread = Some(Box::new(self.expr(true)?));
                continue;
            }
            let pos = self.pos();
            let name = match self.advance().tok {
                Tok::Ident(n) => n,
                Tok::Str(pieces) => match pieces.as_slice() {
                    [Piece::Text(t)] => t.clone(),
                    _ => return Err(ScriptError::new(pos, "a key can't interpolate".to_string())),
                },
                other => {
                    return Err(ScriptError::new(
                        pos,
                        format!("expected a field name, found {}", Self::describe(&other)),
                    ));
                }
            };
            let value = if self.eat_sym(":") {
                self.expr(true)?
            } else {
                // `{ quantity }` is `{ quantity: quantity }`.
                Expr {
                    kind: ExprKind::Var(name.clone()),
                    pos,
                }
            };
            if fields.iter().any(|(f, _): &(String, Expr)| f == &name) {
                return Err(ScriptError::new(
                    pos,
                    format!("field '{}' is given twice", name),
                ));
            }
            fields.push((name, value));
        }
        Ok((fields, spread))
    }

    fn primary(&mut self, s: bool) -> R<Expr> {
        let token = self.advance();
        let pos = token.pos;
        let kind = match token.tok {
            Tok::Int(n) => ExprKind::Int(n),
            Tok::Float(n) => ExprKind::Float(n),
            Tok::Str(pieces) => {
                let mut parts = Vec::new();
                for piece in pieces {
                    match piece {
                        Piece::Text(t) => parts.push(StrPart::Text(t)),
                        Piece::Code(tokens) => {
                            let mut inner = Parser { tokens, i: 0 };
                            let expr = inner.expr(true)?;
                            if !inner.at_eof() {
                                return inner.expected("'}' closing the interpolation");
                            }
                            parts.push(StrPart::Expr(expr));
                        }
                    }
                }
                ExprKind::Str(parts)
            }
            Tok::Sym("(") => {
                let inner = self.expr(true)?;
                self.expect_sym(")")?;
                return Ok(inner);
            }
            Tok::Sym("[") => {
                let mut items = Vec::new();
                while !self.eat_sym("]") {
                    // One item per line needs no comma (like struct fields);
                    // on one line, `[a -1]` would be ambiguous.
                    let new_line = self.i > 0 && self.tokens[self.i - 1].pos.line < self.pos().line;
                    if !items.is_empty() && !self.eat_sym(",") && !new_line {
                        return self.expected("',' or a new line between list items");
                    }
                    if self.eat_sym("]") {
                        break;
                    }
                    items.push(self.expr(true)?);
                }
                ExprKind::List(items)
            }
            Tok::Sym("{") => {
                let (fields, spread) = self.fields()?;
                if spread.is_some() {
                    return Err(ScriptError::new(
                        pos,
                        "'..base' needs a type: Task { done: true, ..task }".to_string(),
                    ));
                }
                ExprKind::Object(fields)
            }
            Tok::Ident(word) => match word.as_str() {
                "true" => ExprKind::Bool(true),
                "false" => ExprKind::Bool(false),
                "none" => ExprKind::None,
                "if" => return self.if_value(pos),
                w if KEYWORDS.contains(&w) => {
                    return Err(ScriptError::new(pos, format!("unexpected '{}'", w)));
                }
                _ => {
                    if self.is_sym("=>") {
                        self.i += 1;
                        let body = self.expr(true)?;
                        ExprKind::Lambda(word, Box::new(body))
                    } else if self.eat_sym("(") {
                        ExprKind::Call(word, self.args()?)
                    } else if s && self.is_sym("{") && word.starts_with(|c: char| c.is_uppercase())
                    {
                        self.i += 1;
                        let (fields, spread) = self.fields()?;
                        ExprKind::Struct {
                            name: word,
                            fields,
                            spread,
                        }
                    } else {
                        ExprKind::Var(word)
                    }
                }
            },
            other => {
                return Err(ScriptError::new(
                    pos,
                    format!("expected a value, found {}", Self::describe(&other)),
                ));
            }
        };
        Ok(Expr { kind, pos })
    }
}
