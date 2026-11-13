//! Syntax tree of a `logic { ... }` script.

use super::Pos;

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let x = e` / `var x: T = e`.
    Let {
        name: String,
        mutable: bool,
        ty: Option<TypeExpr>,
        value: Expr,
        pos: Pos,
    },
    /// `x = e`, `x.field = e` (only on a `var`).
    Assign {
        target: Expr,
        value: Expr,
        pos: Pos,
    },
    If {
        cond: Cond,
        then: Vec<Stmt>,
        otherwise: Option<Vec<Stmt>>,
        pos: Pos,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
        pos: Pos,
    },
    Return {
        value: Expr,
        pos: Pos,
    },
    /// `fail Variant` / `fail Variant("message")`.
    Fail {
        variant: String,
        message: Option<Expr>,
        pos: Pos,
    },
    /// A call run for its effect (`Infra.delete(id)`, `xs.push(x)`).
    Expr {
        expr: Expr,
        pos: Pos,
    },
}

#[derive(Debug, Clone)]
pub enum Cond {
    Bool(Expr),
    /// `if let v = optional { ... }`.
    Let(String, Expr),
}

/// `extern fn name(a: T, b: &U) -> R from "./file.rs"` in a top-level
/// `logic {}` block: a `pub fn` of a Rust file, callable from scripts.
#[derive(Debug, Clone)]
pub struct ExternFn {
    pub name: String,
    /// `(name, type, passed by reference)`.
    pub params: Vec<(String, TypeExpr, bool)>,
    pub ret: TypeExpr,
    pub from: String,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named(String, Pos),
    Generic(String, Vec<TypeExpr>, Pos),
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(String),
    Float(String),
    Str(Vec<StrPart>),
    Bool(bool),
    None,
    Var(String),
    Field(Box<Expr>, String),
    /// `f(args)`.
    Call(String, Vec<Arg>),
    /// `receiver.name(args)`.
    Method(Box<Expr>, String, Vec<Arg>),
    /// `Type { field: e, ..base }`.
    Struct {
        name: String,
        fields: Vec<(String, Expr)>,
        spread: Option<Box<Expr>>,
    },
    /// `{ key: e }` — a JSON object.
    Object(Vec<(String, Expr)>),
    List(Vec<Expr>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `x => e`, only as a list method's argument.
    Lambda(String, Box<Expr>),
    /// `fail Variant(...)` as an expression: the right side of `??`.
    Fail(String, Option<Box<Expr>>),
}

#[derive(Debug, Clone)]
pub enum StrPart {
    Text(String),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Coalesce,
    In,
    NotIn,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::Coalesce => "??",
            BinOp::In => "in",
            BinOp::NotIn => "not in",
        }
    }
}
