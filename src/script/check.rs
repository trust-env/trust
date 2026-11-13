//! Type checks a script and writes the Rust body of its use case's `run`,
//! in one pass.
//!
//! The generated body runs inside `async fn run(&self, user: i64, input:
//! In) -> Result<Out, {Name}Error>`, so:
//! - every infra / use case call gets `.await?` (their errors convert into
//!   the use case's error enum);
//! - values are cloned when read (scripts never think about ownership);
//! - anything that can fail (`?`) is refused inside `x => ...` lambdas,
//!   which become plain Rust closures.

use heck::{ToSnakeCase, ToUpperCamelCase};

use super::ast::*;
use super::types::{InfraKind, Project, Ty};
use super::{Pos, ScriptError};

type R<T> = Result<T, ScriptError>;

fn err<T>(pos: Pos, message: impl Into<String>) -> R<T> {
    Err(ScriptError::new(pos, message.into()))
}

/// Rust keywords a script variable may still be named after (`r#type`).
const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "dyn", "enum", "extern", "fn", "impl", "loop", "match",
    "mod", "move", "mut", "pub", "ref", "static", "struct", "trait", "type", "unsafe", "use",
    "where", "while", "async", "await", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// The use case being written.
pub struct Target<'a> {
    pub name: &'a str,
    pub input: Ty,
    pub output: Ty,
    /// `owner: none` on the use case: psql calls aren't scoped to the
    /// caller.
    pub unscoped: bool,
    /// The use case declares `journal:`: `journal("...")` names its entry.
    pub journal: bool,
}

/// What lowering produced: the body, and what it reaches.
#[derive(Debug, Default)]
pub struct Lowered {
    pub rust: String,
    pub infras: Vec<String>,
    pub use_cases: Vec<String>,
    pub errors: Vec<String>,
    pub deps: Vec<(String, String)>,
    /// Rust files of the `extern fn`s the script calls.
    pub includes: Vec<String>,
}

#[derive(Clone)]
struct Local {
    name: String,
    ty: Ty,
    mutable: bool,
    /// A loop / lambda variable: a `&T` in Rust.
    by_ref: bool,
    rust: String,
}

/// A readable place: `x`, `x.field`, `input.items`.
struct Place {
    ty: Ty,
    code: String,
    /// The local it starts from.
    root: String,
    root_mutable: bool,
    /// The place is the root itself, which is a `&T`.
    bare_ref: bool,
}

pub fn lower(project: &Project, target: &Target, body: &[Stmt]) -> R<Lowered> {
    let mut lowerer = Lowerer {
        p: project,
        t: target,
        error: format!("{}Error", target.name),
        scopes: vec![vec![
            Local {
                name: "user".into(),
                ty: Ty::Int,
                mutable: false,
                by_ref: false,
                rust: "user".into(),
            },
            Local {
                name: "input".into(),
                ty: target.input.clone(),
                mutable: false,
                by_ref: false,
                rust: "input".into(),
            },
        ]],
        out: Lowered::default(),
        uses_try: false,
        lambda: 0,
        iterating: Vec::new(),
    };
    let (code, diverges) = lowerer.block(body, 0)?;
    if !diverges {
        let pos = body.last().map(stmt_pos).unwrap_or(Pos { line: 1, col: 1 });
        return err(
            pos,
            "the logic can reach its end without 'return' or 'fail'",
        );
    }
    lowerer.out.rust = if lowerer.uses_try {
        // `try` turns an error into its text: every variant of the enum.
        let mut arms = vec![
            format!("{}::NotFound => String::from(\"not found\")", lowerer.error),
            format!("{}::StorageError(message) => message", lowerer.error),
        ];
        for variant in &lowerer.out.errors {
            arms.push(format!(
                "{}::{}(message) => message",
                lowerer.error, variant
            ));
        }
        format!(
            "let trust_error_text = |error: {}| -> String {{ match error {{ {} }} }};\n{}",
            lowerer.error,
            arms.join(", "),
            code
        )
    } else {
        code
    };
    Ok(lowerer.out)
}

fn stmt_pos(stmt: &Stmt) -> Pos {
    match stmt {
        Stmt::Let { pos, .. }
        | Stmt::Assign { pos, .. }
        | Stmt::If { pos, .. }
        | Stmt::For { pos, .. }
        | Stmt::Return { pos, .. }
        | Stmt::Fail { pos, .. }
        | Stmt::Expr { pos, .. } => *pos,
    }
}

/// `code` without one pair of parentheses wrapping all of it — for spots
/// that take a whole expression (`let x = ...`, `if ... {`, an argument),
/// where rustc would warn about them.
fn bare(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    if chars.first() != Some(&'(') || chars.last() != Some(&')') {
        return code.to_string();
    }
    let mut depth = 0;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i != chars.len() - 1 {
                    return code.to_string();
                }
            }
            _ => {}
        }
        i += 1;
    }
    chars[1..chars.len() - 1].iter().collect()
}

#[cfg(test)]
pub fn bare_for_tests(code: &str) -> String {
    bare(code)
}

/// Functions every script has (an `extern fn` can't take these names).
pub const BUILTINS: &[&str] = &[
    "now",
    "today",
    "env",
    "sha256",
    "hmac_sha256",
    "secure_eq",
    "to_json",
    "min",
    "max",
    "journal",
    "env_opt",
    "url_encode",
    "parse_datetime",
    "parse_date",
    "week_start",
    "log_warn",
    "log_info",
];

/// A written type (`List<Task>`), resolved against the project.
pub fn resolve_type(p: &Project, t: &TypeExpr) -> R<Ty> {
    match t {
        TypeExpr::Named(name, pos) => Ok(match name.as_str() {
            "Int" => Ty::Int,
            "Float" => Ty::Float,
            "Decimal" => Ty::Decimal,
            "Bool" => Ty::Bool,
            "String" => Ty::Str,
            "Uuid" => Ty::Uuid,
            "DateTime" => Ty::DateTime,
            "Date" => Ty::Date,
            "Time" => Ty::Time,
            "Json" => Ty::Json,
            other => p.named(other).or_else(|e| err(*pos, e))?,
        }),
        TypeExpr::Generic(name, args, pos) => match (name.as_str(), args.as_slice()) {
            ("List", [inner]) => Ok(Ty::list(resolve_type(p, inner)?)),
            ("Option", [inner]) => Ok(Ty::opt(resolve_type(p, inner)?)),
            ("Try", [inner]) => Ok(Ty::Try(Box::new(resolve_type(p, inner)?))),
            ("Map", [key, value]) => {
                let key = resolve_type(p, key)?;
                if !map_key(&key) {
                    return err(
                        *pos,
                        format!(
                            "a Map key is an Int, String, Bool, Date, DateTime or Uuid, not {}",
                            key.name()
                        ),
                    );
                }
                Ok(Ty::Map(Box::new(key), Box::new(resolve_type(p, value)?)))
            }
            _ => err(
                *pos,
                format!("unknown type '{}<...>' (List<T> and Option<T> exist)", name),
            ),
        },
    }
}

/// The `"NAME"` of `env("NAME")`: a literal, so every variable a script
/// reads shows in the source.
fn env_name(arg: &Expr) -> R<String> {
    match &arg.kind {
        ExprKind::Str(parts) => match parts.as_slice() {
            [StrPart::Text(t)]
                if !t.is_empty()
                    && t.chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') =>
            {
                Ok(t.clone())
            }
            _ => err(arg.pos, "env() takes a literal UPPER_CASE name"),
        },
        _ => err(
            arg.pos,
            "env() takes a literal UPPER_CASE name, so every variable a script reads is visible in the source",
        ),
    }
}

/// Whether `ty` can key a `Map` (ordered keys).
fn map_key(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Int | Ty::Str | Ty::Bool | Ty::Date | Ty::DateTime | Ty::Uuid
    )
}

/// A Rust string literal holding `text`.
fn literal(text: &str) -> String {
    format!("{:?}", text)
}

fn variant_ident(wire: &str) -> String {
    let ident = wire.to_upper_camel_case();
    if ident.is_empty() || ident.starts_with(|c: char| c.is_ascii_digit()) {
        format!("V{}", ident)
    } else {
        ident
    }
}

struct Lowerer<'a> {
    p: &'a Project,
    t: &'a Target<'a>,
    error: String,
    scopes: Vec<Vec<Local>>,
    out: Lowered,
    /// A `try` was lowered: the body needs `trust_error_text`.
    uses_try: bool,
    /// Depth of `x => ...` lambdas being lowered.
    lambda: usize,
    /// Roots of the lists `for` loops are walking (can't change meanwhile).
    iterating: Vec<String>,
}

impl Lowerer<'_> {
    // ---- scopes -------------------------------------------------------

    fn local(&self, name: &str) -> Option<&Local> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|l| l.name == name)
    }

    fn declare(&mut self, name: &str, ty: Ty, mutable: bool, by_ref: bool, pos: Pos) -> R<String> {
        if !name.starts_with(|c: char| c.is_lowercase() || c == '_') {
            return err(
                pos,
                format!("variable names start with a lowercase letter ('{}')", name),
            );
        }
        if ["user", "input", "self", "super", "crate", "Self"].contains(&name)
            || name.starts_with("trust_")
        {
            return err(pos, format!("'{}' is reserved", name));
        }
        if self
            .scopes
            .last()
            .is_some_and(|s| s.iter().any(|l| l.name == name))
        {
            return err(
                pos,
                format!(
                    "'{}' is already declared here: pick another name (or use 'var' and assign it)",
                    name
                ),
            );
        }
        let rust = if RUST_KEYWORDS.contains(&name) {
            format!("r#{}", name)
        } else {
            name.to_string()
        };
        self.scopes.last_mut().expect("scope").push(Local {
            name: name.to_string(),
            ty,
            mutable,
            by_ref,
            rust: rust.clone(),
        });
        Ok(rust)
    }

    fn need_try(&self, pos: Pos, what: &str) -> R<()> {
        if self.lambda > 0 {
            return err(
                pos,
                format!(
                    "{} can fail, so it can't run inside 'x => ...': use a 'for' loop",
                    what
                ),
            );
        }
        Ok(())
    }

    fn dep(&mut self, name: &str, version: &str) {
        if !self.out.deps.iter().any(|(n, _)| n == name) {
            self.out.deps.push((name.to_string(), version.to_string()));
        }
    }

    fn fail_code(&mut self, variant: &str, message: Option<&Expr>, pos: Pos) -> R<String> {
        if !variant.starts_with(|c: char| c.is_uppercase()) {
            return err(
                pos,
                format!("error names start with an uppercase letter ('{}')", variant),
            );
        }
        if variant == "NotFound" {
            if message.is_some() {
                return err(pos, "NotFound takes no message");
            }
            return Ok(format!("{}::NotFound", self.error));
        }
        let message = match message {
            Some(m) => self.expect(m, &Ty::Str)?,
            None => format!("String::from({})", literal(variant)),
        };
        if variant != "StorageError" && !self.out.errors.iter().any(|e| e == variant) {
            self.out.errors.push(variant.to_string());
        }
        Ok(format!("{}::{}({})", self.error, variant, message))
    }

    // ---- types --------------------------------------------------------

    fn type_expr(&self, t: &TypeExpr) -> R<Ty> {
        resolve_type(self.p, t)
    }

    fn field_ty(&self, ty: &Ty, field: &str, pos: Pos) -> R<Ty> {
        match ty {
            Ty::Struct(name) => {
                let fields = self.p.fields(name).or_else(|e| err(pos, e))?;
                match fields.iter().find(|(f, _)| f == field) {
                    Some((_, t)) => self.p.ty(t).or_else(|e| err(pos, e)),
                    None => err(
                        pos,
                        format!(
                            "'{}' has no field '{}' (fields: {})",
                            name,
                            field,
                            fields
                                .iter()
                                .map(|(f, _)| f.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    ),
                }
            }
            Ty::Json => Ok(Ty::Json),
            Ty::RawRequest if field == "body" => Ok(Ty::Str),
            Ty::RawRequest => err(pos, "a RawRequest has 'body' and 'header(\"name\")'"),
            Ty::Opt(_) => err(
                pos,
                format!(
                    "this {} may be none: unwrap it first ('if let x = ... {{ }}', '?? default' or '?? fail NotFound')",
                    ty.name()
                ),
            ),
            other => err(pos, format!("a {} has no field '{}'", other.name(), field)),
        }
    }

    /// `code` of type `ty` where `expected` is wanted (`T` fits `Option<T>`).
    fn coerce(&self, ty: &Ty, code: String, expected: &Ty, pos: Pos) -> R<String> {
        if ty == expected || *ty == Ty::Never {
            return Ok(code);
        }
        if let Ty::Opt(inner) = expected
            && **inner == *ty
        {
            return Ok(format!("Some({})", bare(&code)));
        }
        err(
            pos,
            format!("expected {}, found {}", expected.name(), ty.name()),
        )
    }

    fn expect(&mut self, e: &Expr, expected: &Ty) -> R<String> {
        let (ty, code) = self.expr(e, Some(expected))?;
        self.coerce(&ty, code, expected, e.pos)
    }

    // ---- statements ---------------------------------------------------

    fn block(&mut self, stmts: &[Stmt], indent: usize) -> R<(String, bool)> {
        self.scopes.push(Vec::new());
        let result = self.block_inner(stmts, indent);
        self.scopes.pop();
        result
    }

    fn block_inner(&mut self, stmts: &[Stmt], indent: usize) -> R<(String, bool)> {
        let pad = "    ".repeat(indent);
        let mut code = String::new();
        let mut diverges = false;
        for stmt in stmts {
            if diverges {
                return err(
                    stmt_pos(stmt),
                    "unreachable: the statement before always returns or fails",
                );
            }
            let (text, ends) = self.stmt(stmt, indent)?;
            for line in text.lines() {
                code.push_str(&pad);
                code.push_str(line);
                code.push('\n');
            }
            diverges = ends;
        }
        Ok((code, diverges))
    }

    /// One statement, unindented (nested blocks are indented relative to
    /// it), and whether it always returns / fails.
    fn stmt(&mut self, stmt: &Stmt, indent: usize) -> R<(String, bool)> {
        let _ = indent;
        match stmt {
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
                pos,
            } => {
                let annotated = match ty {
                    Some(t) => Some(self.type_expr(t)?),
                    None => None,
                };
                let (vty, code) = self.expr(value, annotated.as_ref())?;
                let (ty, code) = match &annotated {
                    Some(t) => (t.clone(), self.coerce(&vty, code, t, value.pos)?),
                    None => (vty, code),
                };
                match ty {
                    Ty::Unit => return err(value.pos, "this doesn't produce a value"),
                    Ty::Never => return err(value.pos, "'fail' can't be stored"),
                    _ => {}
                }
                let rust = self.declare(name, ty.clone(), *mutable, false, *pos)?;
                let keyword = if *mutable { "let mut" } else { "let" };
                let annotation = if annotated.is_some() {
                    format!(": {}", ty.rust())
                } else {
                    String::new()
                };
                Ok((
                    format!("{} {}{} = {};", keyword, rust, annotation, bare(&code)),
                    false,
                ))
            }
            Stmt::Assign { target, value, pos } => {
                let Some(place) = self.place(target)? else {
                    return err(*pos, "only a 'var' (or one of its fields) can be assigned");
                };
                self.check_mutable(&place, *pos)?;
                let code = self.expect(value, &place.ty)?;
                Ok((format!("{} = {};", place.code, bare(&code)), false))
            }
            Stmt::If {
                cond,
                then,
                otherwise,
                ..
            } => {
                self.scopes.push(Vec::new());
                let head = match cond {
                    Cond::Bool(e) => format!("if {} {{", bare(&self.expect(e, &Ty::Bool)?)),
                    Cond::Variant {
                        ty,
                        variant,
                        bindings,
                        value,
                        pos,
                    } => {
                        let (vty, code) = self.expr(value, None)?;
                        if vty != Ty::Tagged(ty.clone()) {
                            return err(
                                value.pos,
                                format!("expected a {}, found {}", ty, vty.name()),
                            );
                        }
                        let Some(fields) = self.p.variant_fields(ty, variant) else {
                            return err(*pos, format!("'{}' has no value '{}'", ty, variant));
                        };
                        let mut parts = Vec::new();
                        for (field, name) in bindings {
                            let Some((_, declared)) = fields.iter().find(|(f, _)| f == field)
                            else {
                                return err(
                                    *pos,
                                    format!("{}.{} has no field '{}'", ty, variant, field),
                                );
                            };
                            let fty = self.p.ty(declared).or_else(|e| err(*pos, e))?;
                            let rust = self.declare(name, fty, false, false, *pos)?;
                            parts.push(if &rust == field {
                                rust
                            } else {
                                format!("{}: {}", field, rust)
                            });
                        }
                        parts.push("..".to_string());
                        format!(
                            "if let {}::{} {{ {} }} = {} {{",
                            ty,
                            variant_ident(variant),
                            parts.join(", "),
                            code
                        )
                    }
                    Cond::Let(name, e) => {
                        let (ty, code) = self.expr(e, None)?;
                        let Ty::Opt(inner) = ty else {
                            return err(
                                e.pos,
                                format!("'if let' unwraps an Option, found {}", ty.name()),
                            );
                        };
                        let rust = self.declare(name, *inner, false, false, e.pos)?;
                        format!("if let Some({}) = {} {{", rust, code)
                    }
                };
                let then = self.block(then, 1);
                self.scopes.pop();
                let (then_code, then_ends) = then?;
                let mut code = format!("{}\n{}}}", head, then_code);
                let mut ends = false;
                if let Some(otherwise) = otherwise {
                    let (else_code, else_ends) = self.block(otherwise, 1)?;
                    code = format!("{} else {{\n{}}}", code, else_code);
                    ends = then_ends && else_ends;
                }
                Ok((code, ends))
            }
            Stmt::For {
                var,
                iter,
                body,
                pos,
            } => {
                let (ty, list) = self.recv(iter)?;
                let Ty::List(item) = ty else {
                    return err(iter.pos, format!("'for' walks a List, found {}", ty.name()));
                };
                let root = self.place(iter)?.map(|p| p.root);
                self.scopes.push(Vec::new());
                let rust = self.declare(var, *item, false, true, *pos);
                let rust = match rust {
                    Ok(r) => r,
                    Err(e) => {
                        self.scopes.pop();
                        return Err(e);
                    }
                };
                if let Some(root) = &root {
                    self.iterating.push(root.clone());
                }
                let lowered = self.block(body, 1);
                if root.is_some() {
                    self.iterating.pop();
                }
                self.scopes.pop();
                let (body_code, _) = lowered?;
                Ok((
                    format!("for {} in {}.iter() {{\n{}}}", rust, list, body_code),
                    false,
                ))
            }
            Stmt::Return { value, .. } => {
                let output = self.t.output.clone();
                let code = self.expect(value, &output)?;
                Ok((format!("return Ok({});", bare(&code)), true))
            }
            Stmt::Fail {
                variant,
                message,
                pos,
            } => {
                let code = self.fail_code(variant, message.as_ref(), *pos)?;
                Ok((format!("return Err({});", code), true))
            }
            Stmt::Expr { expr, pos } => {
                if !matches!(expr.kind, ExprKind::Method(..) | ExprKind::Call(..)) {
                    return err(
                        *pos,
                        "this value isn't used: store it with 'let', or 'return' it",
                    );
                }
                let (ty, code) = self.expr(expr, None)?;
                if ty == Ty::Unit {
                    Ok((format!("{};", code), false))
                } else {
                    Ok((format!("let _ = {};", code), false))
                }
            }
        }
    }

    fn check_mutable(&self, place: &Place, pos: Pos) -> R<()> {
        if !place.root_mutable {
            return err(
                pos,
                format!("'{}' can't change: declare it with 'var'", place.root),
            );
        }
        if self.iterating.contains(&place.root) {
            return err(
                pos,
                format!("'{}' can't change while a 'for' loop walks it", place.root),
            );
        }
        Ok(())
    }

    // ---- places and values -------------------------------------------

    fn place(&mut self, e: &Expr) -> R<Option<Place>> {
        match &e.kind {
            ExprKind::Var(name) => Ok(self.local(name).map(|l| Place {
                ty: l.ty.clone(),
                code: l.rust.clone(),
                root: l.name.clone(),
                root_mutable: l.mutable,
                bare_ref: l.by_ref,
            })),
            ExprKind::Field(inner, field) => {
                let Some(base) = self.place(inner)? else {
                    return Ok(None);
                };
                if matches!(base.ty, Ty::Try(_) | Ty::Response) {
                    return Ok(None);
                }
                let ty = self.field_ty(&base.ty, field, e.pos)?;
                let code = match base.ty {
                    Ty::Json => format!("{}[{}]", base.code, literal(field)),
                    _ => format!("{}.{}", base.code, field),
                };
                Ok(Some(Place {
                    ty,
                    code,
                    root: base.root,
                    root_mutable: base.root_mutable,
                    bare_ref: false,
                }))
            }
            _ => Ok(None),
        }
    }

    /// An owned value of a place.
    fn place_value(place: &Place) -> String {
        if place.ty.is_copy() {
            if place.bare_ref {
                format!("*{}", place.code)
            } else {
                place.code.clone()
            }
        } else {
            format!("{}.clone()", place.code)
        }
    }

    /// The receiver of a method: a place is borrowed, not cloned.
    fn recv(&mut self, e: &Expr) -> R<(Ty, String)> {
        if let Some(place) = self.place(e)? {
            if place.ty.is_copy() {
                return Ok((place.ty.clone(), Self::place_value(&place)));
            }
            return Ok((place.ty, place.code));
        }
        let (ty, code) = self.expr(e, None)?;
        Ok((ty, format!("({})", code)))
    }

    // ---- expressions --------------------------------------------------

    fn expr(&mut self, e: &Expr, expected: Option<&Ty>) -> R<(Ty, String)> {
        let pos = e.pos;
        let wanted = expected.map(|t| match t {
            Ty::Opt(inner) => (**inner).clone(),
            t => t.clone(),
        });
        match &e.kind {
            ExprKind::Int(n) => Ok(match wanted {
                Some(Ty::Float) => (Ty::Float, format!("{}f64", n)),
                Some(Ty::Decimal) => (
                    Ty::Decimal,
                    format!("rust_decimal::Decimal::from({}i64)", n),
                ),
                _ => {
                    if n.parse::<i64>().is_err() {
                        return err(pos, format!("{} doesn't fit in an Int", n));
                    }
                    (Ty::Int, format!("{}i64", n))
                }
            }),
            ExprKind::Float(n) => Ok((Ty::Float, format!("{}f64", n))),
            ExprKind::Bool(b) => Ok((Ty::Bool, b.to_string())),
            ExprKind::None => match expected {
                Some(t @ Ty::Opt(_)) => Ok((t.clone(), "None".into())),
                _ => err(
                    pos,
                    "the type of 'none' is unknown here: annotate it ('let x: Option<Int> = none')",
                ),
            },
            ExprKind::Str(parts) => self.string(parts),
            ExprKind::Var(name) => {
                if let Some(place) = self.place(e)? {
                    return Ok((place.ty.clone(), Self::place_value(&place)));
                }
                self.unknown_name(name, pos)
            }
            ExprKind::Field(inner, field) => {
                if let Some(place) = self.place(e)? {
                    return Ok((place.ty.clone(), Self::place_value(&place)));
                }
                if let ExprKind::Var(type_name) = &inner.kind
                    && self.local(type_name).is_none()
                {
                    return self.enum_value(type_name, field, pos);
                }
                let (ty, code) = self.recv(inner)?;
                match (&ty, field.as_str()) {
                    (Ty::Try(t), "ok") => {
                        return Ok((Ty::opt((**t).clone()), format!("{}.clone().ok()", code)));
                    }
                    (Ty::Try(_), "error") => {
                        return Ok((Ty::opt(Ty::Str), format!("{}.clone().err()", code)));
                    }
                    (Ty::Try(_), _) => {
                        return err(pos, "a Try has 'ok', 'error', is_ok() and is_error()");
                    }
                    (Ty::Response, "status") => return Ok((Ty::Int, format!("{}.0", code))),
                    (Ty::Response, "text") => return Ok((Ty::Str, format!("{}.1.clone()", code))),
                    (Ty::Response, _) => {
                        return err(pos, "a Response has 'status', 'text' and json()");
                    }
                    _ => {}
                }
                let fty = self.field_ty(&ty, field, pos)?;
                let value = match ty {
                    Ty::Json => format!("{}[{}].clone()", code, literal(field)),
                    _ if fty.is_copy() => format!("{}.{}", code, field),
                    _ => format!("{}.{}.clone()", code, field),
                };
                Ok((fty, value))
            }
            ExprKind::Call(name, args) => self.call(name, args, pos),
            ExprKind::Method(recv, name, args) => self.method(recv, name, args, expected, pos),
            ExprKind::Struct {
                name,
                fields,
                spread,
            } => self.struct_literal(name, fields, spread.as_deref(), pos),
            ExprKind::Object(fields)
                if fields.is_empty() && matches!(expected, Some(Ty::Map(..))) =>
            {
                let ty = expected.cloned().expect("a Map");
                let Ty::Map(k, v) = &ty else { unreachable!() };
                let code = format!(
                    "std::collections::BTreeMap::<{}, {}>::new()",
                    k.rust(),
                    v.rust()
                );
                Ok((ty.clone(), code))
            }
            ExprKind::Object(fields) => {
                let mut code = String::from("{ let mut trust_object = serde_json::Map::new(); ");
                for (key, value) in fields {
                    let (ty, value_code) = self.expr(value, None)?;
                    if matches!(ty, Ty::Unit | Ty::Never) {
                        return err(value.pos, "this doesn't produce a value");
                    }
                    code.push_str(&format!(
                        "trust_object.insert(String::from({}), serde_json::to_value({}).unwrap_or_default()); ",
                        literal(key),
                        bare(&value_code)
                    ));
                }
                code.push_str("serde_json::Value::Object(trust_object) }");
                Ok((Ty::Json, code))
            }
            ExprKind::List(items) => {
                let item_expected = match &wanted {
                    Some(Ty::List(item)) => Some((**item).clone()),
                    _ => None,
                };
                if items.is_empty() {
                    return match item_expected {
                        Some(item) => Ok((
                            Ty::list(item.clone()),
                            format!("Vec::<{}>::new()", item.rust()),
                        )),
                        None => err(
                            pos,
                            "the type of '[]' is unknown here: annotate it ('let xs: List<Int> = []')",
                        ),
                    };
                }
                let mut codes = Vec::new();
                let mut item_ty = item_expected;
                for item in items {
                    match &item_ty {
                        Some(t) => {
                            let t = t.clone();
                            codes.push(bare(&self.expect(item, &t)?));
                        }
                        None => {
                            let (t, code) = self.expr(item, None)?;
                            codes.push(bare(&code));
                            item_ty = Some(t);
                        }
                    }
                }
                let item_ty = item_ty.expect("non-empty list");
                Ok((Ty::list(item_ty), format!("vec![{}]", codes.join(", "))))
            }
            ExprKind::Neg(inner) => {
                let (ty, code) = self.expr(inner, wanted.as_ref())?;
                if !ty.is_numeric() {
                    return err(pos, format!("'-' needs a number, found {}", ty.name()));
                }
                Ok((ty, format!("(-{})", code)))
            }
            ExprKind::Not(inner) => {
                Ok((Ty::Bool, format!("(!{})", self.expect(inner, &Ty::Bool)?)))
            }
            ExprKind::Binary(op, left, right) => self.binary(*op, left, right, expected, pos),
            ExprKind::Lambda(..) => err(
                pos,
                "'x => ...' only goes inside a list method: xs.map(x => x.title)",
            ),
            ExprKind::Fail(..) => err(pos, "'fail' here only works after '??'"),
            ExprKind::Variant {
                ty,
                variant,
                fields,
            } => self.variant_value(ty, variant, fields, pos),
            ExprKind::If(cond, then, otherwise) => {
                let cond = bare(&self.expect(cond, &Ty::Bool)?);
                // A `none` / `[]` / number branch takes the other one's type.
                let literal = |e: &Expr| {
                    matches!(e.kind, ExprKind::None | ExprKind::Int(_))
                        || matches!(&e.kind, ExprKind::List(i) if i.is_empty())
                };
                // `none` against a `T` is an `Option<T>`.
                let fit = |e: &Expr, other: &Ty| match (&e.kind, other) {
                    (ExprKind::None, Ty::Opt(_)) | (_, Ty::Never) => other.clone(),
                    (ExprKind::None, t) => Ty::opt(t.clone()),
                    (_, t) => t.clone(),
                };
                let (l, r) = if literal(then) {
                    let r = self.expr(otherwise, expected)?;
                    (self.expr(then, Some(&fit(then, &r.0)))?, r)
                } else {
                    let l = self.expr(then, expected)?;
                    let r = self.expr(otherwise, Some(&fit(otherwise, &l.0)))?;
                    (l, r)
                };
                let (ty, a, b) = if l.0 == Ty::Never {
                    (r.0, l.1, r.1)
                } else if r.0 == Ty::Never {
                    (l.0, l.1, r.1)
                } else {
                    self.unify(l, r, pos, BinOp::Eq).map_err(|_| {
                        ScriptError::new(
                            pos,
                            "both branches of an 'if' value need the same type".to_string(),
                        )
                    })?
                };
                if matches!(ty, Ty::Unit) {
                    return err(pos, "an 'if' value needs values in its branches");
                }
                Ok((
                    ty,
                    format!("(if {} {{ {} }} else {{ {} }})", cond, bare(&a), bare(&b)),
                ))
            }
            ExprKind::Try(inner) => {
                self.need_try(pos, "'try'")?;
                // `let r: Try<T> = try ...` and `try ... ?? fallback` decode into T.
                let inner_expected = match expected {
                    Some(Ty::Try(t)) | Some(Ty::Opt(t)) => Some((**t).clone()),
                    _ => None,
                };
                let (ty, code) = self.expr(inner, inner_expected.as_ref())?;
                if matches!(ty, Ty::Unit | Ty::Never | Ty::Try(_)) {
                    return err(
                        inner.pos,
                        format!("'try' needs a value, found {}", ty.name()),
                    );
                }
                self.uses_try = true;
                Ok((
                    Ty::Try(Box::new(ty)),
                    format!(
                        "async {{ Ok::<_, {}>({}) }}.await.map_err(trust_error_text)",
                        self.error,
                        bare(&code)
                    ),
                ))
            }
        }
    }

    fn unknown_name<T>(&self, name: &str, pos: Pos) -> R<T> {
        if self.p.infras.contains_key(name) {
            return err(
                pos,
                format!(
                    "'{}' is an infra: call one of its methods ({}.find(id), ...)",
                    name, name
                ),
            );
        }
        if self.p.use_cases.contains_key(name) {
            return err(
                pos,
                format!("'{}' is a use case: run it with {}.run(input)", name, name),
            );
        }
        if self.p.types.contains_key(name) {
            return err(
                pos,
                format!(
                    "'{}' is a type: build one with {} {{ field: value }}",
                    name, name
                ),
            );
        }
        err(pos, format!("unknown name '{}'", name))
    }

    fn enum_value(&mut self, type_name: &str, variant: &str, pos: Pos) -> R<(Ty, String)> {
        match self.p.named(type_name) {
            Ok(Ty::Enum(name)) => {
                let variants = self.p.enum_variants(&name);
                if !variants.iter().any(|v| v == variant) {
                    return err(
                        pos,
                        format!(
                            "'{}' has no value '{}' (values: {})",
                            name,
                            variant,
                            variants.join(", ")
                        ),
                    );
                }
                Ok((
                    Ty::Enum(name.clone()),
                    format!("{}::{}", name, variant_ident(variant)),
                ))
            }
            Ok(Ty::Tagged(name)) => self.variant_value(&name, variant, &[], pos),
            _ => self.unknown_name(type_name, pos),
        }
    }

    /// `Change.move { start: s, end: e }` (or `Change.reset`) of a tagged
    /// enum: every field given, except Options and Lists (empty).
    fn variant_value(
        &mut self,
        ty: &str,
        variant: &str,
        given: &[(String, Expr)],
        pos: Pos,
    ) -> R<(Ty, String)> {
        let enum_ty = match self.p.named(ty) {
            Ok(t @ Ty::Tagged(_)) => t,
            Ok(Ty::Enum(_)) => {
                return err(
                    pos,
                    format!("'{}' values carry no fields: write {}.{}", ty, ty, variant),
                );
            }
            _ => return self.unknown_name(ty, pos),
        };
        let Some(fields) = self.p.variant_fields(ty, variant) else {
            return err(
                pos,
                format!(
                    "'{}' has no value '{}' (values: {})",
                    ty,
                    variant,
                    self.p.enum_variants(ty).join(", ")
                ),
            );
        };
        let ident = format!("{}::{}", ty, variant_ident(variant));
        if fields.is_empty() {
            if !given.is_empty() {
                return err(pos, format!("{}.{} carries no fields", ty, variant));
            }
            return Ok((enum_ty, ident));
        }
        let mut parts = Vec::new();
        for (field, value) in given {
            let Some((_, declared)) = fields.iter().find(|(f, _)| f == field) else {
                return err(
                    value.pos,
                    format!("{}.{} has no field '{}'", ty, variant, field),
                );
            };
            let fty = self.p.ty(declared).or_else(|e| err(pos, e))?;
            let code = self.expect(value, &fty)?;
            parts.push(format!("{}: {}", field, bare(&code)));
        }
        let mut missing = Vec::new();
        for (field, declared) in &fields {
            if given.iter().any(|(g, _)| g == field) {
                continue;
            }
            match self.p.ty(declared) {
                Ok(Ty::Opt(_)) => parts.push(format!("{}: None", field)),
                Ok(Ty::List(_)) => parts.push(format!("{}: Vec::new()", field)),
                _ => missing.push(field.as_str()),
            }
        }
        if !missing.is_empty() {
            return err(
                pos,
                format!("{}.{} is missing {}", ty, variant, missing.join(", ")),
            );
        }
        Ok((enum_ty, format!("{} {{ {} }}", ident, parts.join(", "))))
    }

    fn string(&mut self, parts: &[StrPart]) -> R<(Ty, String)> {
        let mut format = String::new();
        let mut args = Vec::new();
        for part in parts {
            match part {
                StrPart::Text(t) => format.push_str(&t.replace('{', "{{").replace('}', "}}")),
                StrPart::Expr(e) => {
                    let (ty, code) = self.expr(e, None)?;
                    if !ty.is_display() {
                        let hint = if matches!(ty, Ty::Opt(_)) {
                            ": give it a default with '??'"
                        } else {
                            ""
                        };
                        return err(
                            e.pos,
                            format!("a {} can't be written in a string{}", ty.name(), hint),
                        );
                    }
                    format.push_str("{}");
                    args.push(bare(&code));
                }
            }
        }
        if args.is_empty() {
            return Ok((
                Ty::Str,
                format!(
                    "String::from({})",
                    literal(&format.replace("{{", "{").replace("}}", "}"))
                ),
            ));
        }
        Ok((
            Ty::Str,
            format!("format!({}, {})", literal(&format), args.join(", ")),
        ))
    }

    fn struct_literal(
        &mut self,
        name: &str,
        given: &[(String, Expr)],
        spread: Option<&Expr>,
        pos: Pos,
    ) -> R<(Ty, String)> {
        let ty = self.p.named(name).or_else(|e| err(pos, e))?;
        if !matches!(ty, Ty::Struct(_)) {
            return err(pos, format!("'{}' isn't a struct", name));
        }
        let fields = self.p.fields(name).or_else(|e| err(pos, e))?;
        let mut parts = Vec::new();
        for (field, value) in given {
            let Some((_, declared)) = fields.iter().find(|(f, _)| f == field) else {
                return err(
                    value.pos,
                    format!(
                        "'{}' has no field '{}' (fields: {})",
                        name,
                        field,
                        fields
                            .iter()
                            .map(|(f, _)| f.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            };
            let fty = self.p.ty(declared).or_else(|e| err(pos, e))?;
            let code = self.expect(value, &fty)?;
            parts.push(format!("{}: {}", field, bare(&code)));
        }
        let rest = match spread {
            Some(base) => Some(self.expect(base, &ty)?),
            None => {
                let mut missing = Vec::new();
                for (field, declared) in &fields {
                    if given.iter().any(|(g, _)| g == field)
                        || field == "id"
                        || self.p.is_owner_field(name, field)
                    {
                        continue;
                    }
                    if !matches!(self.p.ty(declared), Ok(Ty::Opt(_)) | Ok(Ty::List(_))) {
                        missing.push(field.as_str());
                    }
                }
                if !missing.is_empty() {
                    return err(
                        pos,
                        format!(
                            "{} is missing {} (only Option, List, id and owner fields may be left out)",
                            name,
                            missing.join(", ")
                        ),
                    );
                }
                if given.len() < fields.len() {
                    Some("Default::default()".to_string())
                } else {
                    None
                }
            }
        };
        if let Some(rest) = rest {
            parts.push(format!("..{}", rest));
        }
        Ok((ty, format!("{} {{ {} }}", name, parts.join(", "))))
    }

    fn operands(&mut self, left: &Expr, right: &Expr) -> R<((Ty, String), (Ty, String))> {
        // A literal on the left takes its type from the right (`0 < x`).
        let literal_left = matches!(left.kind, ExprKind::Int(_) | ExprKind::None)
            || matches!(&left.kind, ExprKind::List(i) if i.is_empty());
        if literal_left {
            let r = self.expr(right, None)?;
            let l = self.expr(left, Some(&r.0))?;
            Ok((l, r))
        } else {
            let l = self.expr(left, None)?;
            let r = self.expr(right, Some(&l.0))?;
            Ok((l, r))
        }
    }

    /// Both sides as one type (`T` and `Option<T>` meet at `Option<T>`).
    fn unify(
        &self,
        l: (Ty, String),
        r: (Ty, String),
        pos: Pos,
        op: BinOp,
    ) -> R<(Ty, String, String)> {
        if l.0 == r.0 {
            return Ok((l.0, l.1, r.1));
        }
        if let Ty::Opt(inner) = &l.0
            && **inner == r.0
        {
            return Ok((l.0.clone(), l.1, format!("Some({})", r.1)));
        }
        if let Ty::Opt(inner) = &r.0
            && **inner == l.0
        {
            return Ok((r.0.clone(), format!("Some({})", l.1), r.1));
        }
        err(
            pos,
            format!(
                "'{}' between {} and {}",
                op.symbol(),
                l.0.name(),
                r.0.name()
            ),
        )
    }

    fn binary(
        &mut self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        expected: Option<&Ty>,
        pos: Pos,
    ) -> R<(Ty, String)> {
        match op {
            BinOp::And | BinOp::Or => {
                let l = self.expect(left, &Ty::Bool)?;
                let r = self.expect(right, &Ty::Bool)?;
                Ok((
                    Ty::Bool,
                    format!(
                        "({} {} {})",
                        l,
                        if op == BinOp::And { "&&" } else { "||" },
                        r
                    ),
                ))
            }
            BinOp::Coalesce => {
                let (lty, lcode) =
                    self.expr(left, expected.map(|t| Ty::opt(t.clone())).as_ref())?;
                // `try e ?? fallback`: the value, or the fallback on failure.
                let (lty, lcode) = match lty {
                    Ty::Try(t) => (Ty::Opt(t), format!("{}.ok()", lcode)),
                    other => (other, lcode),
                };
                let Ty::Opt(inner) = lty else {
                    return err(
                        pos,
                        format!("'??' needs an Option on its left, found {}", lty.name()),
                    );
                };
                let fallback = match &right.kind {
                    ExprKind::Fail(variant, message) => {
                        self.need_try(right.pos, "'fail'")?;
                        format!(
                            "return Err({})",
                            self.fail_code(variant, message.as_deref(), right.pos)?
                        )
                    }
                    _ => {
                        let (rty, rcode) = self.expr(right, Some(&inner))?;
                        // `a ?? b` between two Options is still an Option.
                        if rty == Ty::opt((*inner).clone()) {
                            return Ok((
                                rty,
                                format!(
                                    "match {} {{ Some(trust_value) => Some(trust_value), None => {} }}",
                                    lcode,
                                    bare(&rcode)
                                ),
                            ));
                        }
                        bare(&self.coerce(&rty, rcode, &inner, right.pos)?)
                    }
                };
                Ok((
                    *inner,
                    format!(
                        "match {} {{ Some(trust_value) => trust_value, None => {} }}",
                        lcode, fallback
                    ),
                ))
            }
            BinOp::In | BinOp::NotIn => {
                let (lty, lcode) = self.expr(left, None)?;
                let list_ty = Ty::list(lty.clone());
                let (rty, rcode) = if matches!(right.kind, ExprKind::List(_)) {
                    let code = self.expect(right, &list_ty)?;
                    (list_ty.clone(), format!("({})", code))
                } else {
                    self.recv(right)?
                };
                if rty != list_ty {
                    return err(
                        pos,
                        format!(
                            "'{}' needs a {} on its right, found {}",
                            op.symbol(),
                            list_ty.name(),
                            rty.name()
                        ),
                    );
                }
                if !lty.is_eq() {
                    return err(pos, format!("{} values can't be compared", lty.name()));
                }
                let not = if op == BinOp::NotIn { "!" } else { "" };
                Ok((Ty::Bool, format!("{}{}.contains(&{})", not, rcode, lcode)))
            }
            BinOp::Eq | BinOp::Ne => {
                let (l, r) = self.operands(left, right)?;
                let (ty, l, r) = self.unify(l, r, pos, op)?;
                if !ty.is_eq() {
                    return err(
                        pos,
                        format!(
                            "{} values can't be compared with '{}'",
                            ty.name(),
                            op.symbol()
                        ),
                    );
                }
                Ok((Ty::Bool, format!("({} {} {})", l, op.symbol(), r)))
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let (l, r) = self.operands(left, right)?;
                if matches!(l.0, Ty::Opt(_)) || matches!(r.0, Ty::Opt(_)) {
                    return err(
                        pos,
                        format!(
                            "'{}' on an Option: unwrap it first ('?? default' or 'if let')",
                            op.symbol()
                        ),
                    );
                }
                let (ty, l, r) = self.unify(l, r, pos, op)?;
                if !ty.is_ord() {
                    return err(pos, format!("{} values have no order", ty.name()));
                }
                Ok((Ty::Bool, format!("({} {} {})", l, op.symbol(), r)))
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                let (l, r) = self.operands(left, right)?;
                if l.0 != r.0 {
                    return err(
                        pos,
                        format!(
                            "'{}' between {} and {} (convert with .to_float())",
                            op.symbol(),
                            l.0.name(),
                            r.0.name()
                        ),
                    );
                }
                let ty = l.0;
                if op == BinOp::Add && ty == Ty::Str {
                    return Ok((Ty::Str, format!("format!(\"{{}}{{}}\", {}, {})", l.1, r.1)));
                }
                if !ty.is_numeric() {
                    return err(
                        pos,
                        format!("'{}' needs numbers, found {}", op.symbol(), ty.name()),
                    );
                }
                if matches!(op, BinOp::Div | BinOp::Rem) && ty != Ty::Float {
                    self.need_try(pos, "an Int / Decimal division (by zero)")?;
                    let method = if op == BinOp::Div {
                        "checked_div"
                    } else {
                        "checked_rem"
                    };
                    return Ok((
                        ty,
                        format!(
                            "{}.{}({}).ok_or_else(|| {}::StorageError(String::from(\"division by zero\")))?",
                            l.1,
                            method,
                            bare(&r.1),
                            self.error
                        ),
                    ));
                }
                Ok((ty, format!("({} {} {})", l.1, op.symbol(), r.1)))
            }
        }
    }

    // ---- calls --------------------------------------------------------

    fn positional<'e>(
        &self,
        args: &'e [Arg],
        count: usize,
        signature: &str,
        pos: Pos,
    ) -> R<Vec<&'e Expr>> {
        if args.len() != count || args.iter().any(|a| a.name.is_some()) {
            return err(pos, format!("expected {}", signature));
        }
        Ok(args.iter().map(|a| &a.value).collect())
    }

    fn call(&mut self, name: &str, args: &[Arg], pos: Pos) -> R<(Ty, String)> {
        match name {
            "now" => {
                self.positional(args, 0, "now()", pos)?;
                self.dep("chrono", "0.4");
                Ok((Ty::DateTime, "chrono::Utc::now()".into()))
            }
            "today" => {
                self.positional(args, 0, "today()", pos)?;
                self.dep("chrono", "0.4");
                Ok((Ty::Date, self.today()))
            }
            "journal_entry" => {
                if !self.t.journal {
                    return err(
                        pos,
                        "journal_entry() starts an entry of a use case with 'journal: <Journal>'",
                    );
                }
                let text = match args.first() {
                    Some(Arg { name: None, value }) => bare(&self.expect(value, &Ty::Str)?),
                    _ => {
                        return err(
                            pos,
                            "expected journal_entry(text) or journal_entry(text, requires_ack: true)",
                        );
                    }
                };
                let mut code = format!("self.trust_journal.entry({})", text);
                for arg in &args[1..] {
                    match arg.name.as_deref() {
                        Some("requires_ack") => {
                            let ack = bare(&self.expect(&arg.value, &Ty::Bool)?);
                            code.push_str(&format!(".requires_ack({})", ack));
                        }
                        _ => return err(arg.value.pos, "journal_entry() options: requires_ack"),
                    }
                }
                Ok((Ty::Unit, code))
            }
            "journal" => {
                let [text] = self.positional(args, 1, "journal(text)", pos)?[..] else {
                    unreachable!()
                };
                if !self.t.journal {
                    return err(
                        pos,
                        "journal() names the entry of a use case with 'journal: <Journal>'",
                    );
                }
                let text = bare(&self.expect(text, &Ty::Str)?);
                Ok((Ty::Unit, format!("self.trust_journal.text({})", text)))
            }
            "env" => {
                let [arg] = self.positional(args, 1, "env(\"NAME\")", pos)?[..] else {
                    unreachable!()
                };
                let var = env_name(arg)?;
                self.need_try(pos, "env()")?;
                Ok((
                    Ty::Str,
                    format!(
                        "std::env::var({}).map_err(|_| {}::StorageError(String::from({})))?",
                        literal(&var),
                        self.error,
                        literal(&format!("environment variable {} is not set", var))
                    ),
                ))
            }
            "env_opt" => {
                let [arg] = self.positional(args, 1, "env_opt(\"NAME\")", pos)?[..] else {
                    unreachable!()
                };
                let var = env_name(arg)?;
                Ok((
                    Ty::opt(Ty::Str),
                    format!(
                        "std::env::var({}).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())",
                        literal(&var)
                    ),
                ))
            }
            "url_encode" => {
                let [text] = self.positional(args, 1, "url_encode(text)", pos)?[..] else {
                    unreachable!()
                };
                // A variable is lent, not cloned.
                let text = match self.place(text)? {
                    Some(place) if place.ty == Ty::Str => {
                        if place.bare_ref {
                            place.code
                        } else {
                            format!("&{}", place.code)
                        }
                    }
                    _ => format!("&{}", self.expect(text, &Ty::Str)?),
                };
                self.dep("urlencoding", "2");
                Ok((
                    Ty::Str,
                    format!("urlencoding::encode({}).into_owned()", text),
                ))
            }
            "parse_datetime" | "parse_date" => {
                let [text] = self.positional(args, 1, &format!("{}(text)", name), pos)?[..] else {
                    unreachable!()
                };
                let text = bare(&self.expect(text, &Ty::Str)?);
                self.dep("chrono", "0.4");
                let date = "chrono::NaiveDate::parse_from_str(trust_text, \"%Y-%m-%d\").ok()";
                if name == "parse_date" {
                    return Ok((
                        Ty::opt(Ty::Date),
                        format!(
                            "{{ let trust_owned = {}; let trust_text = trust_owned.trim(); chrono::DateTime::parse_from_rfc3339(trust_text).ok().map(|d| d.date_naive()).or_else(|| {}) }}",
                            text, date
                        ),
                    ));
                }
                // A date alone is midnight in the app's time zone.
                let midnight = if self.p.local_time {
                    "crate::tz::midnight(d)"
                } else {
                    "d.and_time(chrono::NaiveTime::MIN).and_utc()"
                };
                Ok((
                    Ty::opt(Ty::DateTime),
                    format!(
                        "{{ let trust_owned = {}; let trust_text = trust_owned.trim(); chrono::DateTime::parse_from_rfc3339(trust_text).ok().map(|d| d.with_timezone(&chrono::Utc)).or_else(|| {}.map(|d| {})) }}",
                        text, date, midnight
                    ),
                ))
            }
            "week_start" => {
                self.positional(args, 0, "week_start()", pos)?;
                self.dep("chrono", "0.4");
                // Monday 0:00 of this week, in the app's time zone.
                let code = if self.p.local_time {
                    "crate::tz::week_bounds(chrono::Utc::now()).0".to_string()
                } else {
                    "{ let trust_today = chrono::Utc::now().date_naive(); (trust_today - chrono::Duration::days(chrono::Datelike::weekday(&trust_today).num_days_from_monday() as i64)).and_time(chrono::NaiveTime::MIN).and_utc() }".to_string()
                };
                Ok((Ty::DateTime, code))
            }
            "log_warn" | "log_info" => {
                let [text] = self.positional(args, 1, &format!("{}(text)", name), pos)?[..] else {
                    unreachable!()
                };
                let text = bare(&self.expect(text, &Ty::Str)?);
                self.dep("tracing", "0.1");
                let level = name.trim_start_matches("log_");
                Ok((
                    Ty::Unit,
                    format!(
                        "tracing::{}!(use_case = {}, \"{{}}\", {})",
                        level,
                        literal(self.t.name),
                        text
                    ),
                ))
            }
            "sha256" => {
                let [text] = self.positional(args, 1, "sha256(text)", pos)?[..] else {
                    unreachable!()
                };
                let text = self.expect(text, &Ty::Str)?;
                self.dep("sha2", "0.10");
                self.dep("hex", "0.4");
                Ok((
                    Ty::Str,
                    format!(
                        "hex::encode(<sha2::Sha256 as sha2::Digest>::digest({}.as_bytes()))",
                        text
                    ),
                ))
            }
            "hmac_sha256" => {
                let [key, message] =
                    self.positional(args, 2, "hmac_sha256(key, message)", pos)?[..]
                else {
                    unreachable!()
                };
                let key = self.expect(key, &Ty::Str)?;
                let message = self.expect(message, &Ty::Str)?;
                self.need_try(pos, "hmac_sha256()")?;
                self.dep("hmac", "0.12");
                self.dep("sha2", "0.10");
                self.dep("hex", "0.4");
                Ok((
                    Ty::Str,
                    format!(
                        "{{ let mut trust_mac = <hmac::Hmac<sha2::Sha256> as hmac::Mac>::new_from_slice({}.as_bytes()).map_err(|e| {}::StorageError(e.to_string()))?; hmac::Mac::update(&mut trust_mac, {}.as_bytes()); hex::encode(hmac::Mac::finalize(trust_mac).into_bytes()) }}",
                        key, self.error, message
                    ),
                ))
            }
            "secure_eq" => {
                let [a, b] = self.positional(args, 2, "secure_eq(a, b)", pos)?[..] else {
                    unreachable!()
                };
                let a = self.expect(a, &Ty::Str)?;
                let b = self.expect(b, &Ty::Str)?;
                // Constant time: compares every byte whatever differs.
                Ok((
                    Ty::Bool,
                    format!(
                        "{{ let (trust_a, trust_b) = ({}, {}); trust_a.len() == trust_b.len() && trust_a.bytes().zip(trust_b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0 }}",
                        a, b
                    ),
                ))
            }
            "to_json" => {
                let [value] = self.positional(args, 1, "to_json(value)", pos)?[..] else {
                    unreachable!()
                };
                let (ty, code) = self.expr(value, None)?;
                if matches!(ty, Ty::Unit | Ty::Never) {
                    return err(value.pos, "this doesn't produce a value");
                }
                Ok((
                    Ty::Json,
                    format!("serde_json::to_value({}).unwrap_or_default()", bare(&code)),
                ))
            }
            "min" | "max" => {
                let [a, b] = self.positional(args, 2, &format!("{}(a, b)", name), pos)?[..] else {
                    unreachable!()
                };
                let (l, r) = self.operands(a, b)?;
                if l.0 != r.0 || !l.0.is_ord() || matches!(l.0, Ty::Opt(_)) {
                    return err(
                        pos,
                        format!(
                            "{}() takes two values of one ordered type, found {} and {}",
                            name,
                            l.0.name(),
                            r.0.name()
                        ),
                    );
                }
                let code = if l.0 == Ty::Float {
                    format!("f64::{}({}, {})", name, bare(&l.1), bare(&r.1))
                } else {
                    format!("std::cmp::{}({}, {})", name, bare(&l.1), bare(&r.1))
                };
                Ok((l.0, code))
            }
            _ if self.p.externs.contains_key(name) => {
                let ext = self.p.externs[name].clone();
                if args.len() != ext.params.len() || args.iter().any(|a| a.name.is_some()) {
                    let sig = ext
                        .params
                        .iter()
                        .map(|(t, r)| format!("{}{}", if *r { "&" } else { "" }, t.name()))
                        .collect::<Vec<_>>();
                    return err(pos, format!("expected {}({})", name, sig.join(", ")));
                }
                let mut codes = Vec::new();
                for (arg, (ty, by_ref)) in args.iter().zip(&ext.params) {
                    if *by_ref {
                        // A place is lent as is, anything else as a temporary.
                        match self.place(&arg.value)? {
                            Some(place) if place.ty == *ty => codes.push(if place.bare_ref {
                                place.code
                            } else {
                                format!("&{}", place.code)
                            }),
                            _ => codes.push(format!("&{}", self.expect(&arg.value, ty)?)),
                        }
                    } else {
                        codes.push(bare(&self.expect(&arg.value, ty)?));
                    }
                }
                if !self.out.includes.contains(&ext.file) {
                    self.out.includes.push(ext.file.clone());
                }
                Ok((ext.ret, format!("crate::{}({})", name, codes.join(", "))))
            }
            _ if self.p.types.contains_key(name) => err(
                pos,
                format!(
                    "'{}' is a type: build one with {} {{ field: value }}",
                    name, name
                ),
            ),
            _ => err(
                pos,
                format!(
                    "unknown function '{}' (functions: now, today, env, sha256, hmac_sha256, secure_eq, to_json, min, max)",
                    name
                ),
            ),
        }
    }

    fn method(
        &mut self,
        recv: &Expr,
        name: &str,
        args: &[Arg],
        expected: Option<&Ty>,
        pos: Pos,
    ) -> R<(Ty, String)> {
        if let ExprKind::Var(target) = &recv.kind
            && self.local(target).is_none()
        {
            if let Some(kind) = self.p.infras.get(target).cloned() {
                return self.infra_call(target, &kind, name, args, expected, pos);
            }
            if self.p.use_cases.contains_key(target) {
                return self.use_case_call(target, name, args, pos);
            }
            if target.starts_with(|c: char| c.is_uppercase()) {
                return self.unknown_name(target, recv.pos);
            }
        }
        self.value_method(recv, name, args, pos)
    }

    fn use_case_call(
        &mut self,
        target: &str,
        name: &str,
        args: &[Arg],
        pos: Pos,
    ) -> R<(Ty, String)> {
        if name != "run" {
            return err(
                pos,
                format!("a use case only has run(input): {}.run(...)", target),
            );
        }
        if target == self.t.name {
            return err(pos, "a use case can't run itself");
        }
        let (input, output) = self.p.use_cases[target].clone();
        let (Some(input), Some(output)) = (input, output) else {
            return err(
                pos,
                format!(
                    "declare 'input' and 'output' on {} to run it from logic",
                    target
                ),
            );
        };
        let input = self.p.ty(&input).or_else(|e| err(pos, e))?;
        let output = self.p.ty(&output).or_else(|e| err(pos, e))?;
        let [arg] =
            self.positional(args, 1, &format!("{}.run({})", target, input.name()), pos)?[..]
        else {
            unreachable!()
        };
        let arg = bare(&self.expect(arg, &input)?);
        self.need_try(pos, "running a use case")?;
        if !self.out.use_cases.iter().any(|u| u == target) {
            self.out.use_cases.push(target.to_string());
        }
        Ok((
            output,
            format!(
                "self.{}.execute(user, {}).await?",
                target.to_snake_case(),
                arg
            ),
        ))
    }

    fn infra_call(
        &mut self,
        infra: &str,
        kind: &InfraKind,
        method: &str,
        args: &[Arg],
        expected: Option<&Ty>,
        pos: Pos,
    ) -> R<(Ty, String)> {
        let field = format!("self.{}", infra.to_snake_case());
        let code = match kind {
            InfraKind::Psql { entity, owner } => {
                let owner = if self.t.unscoped { None } else { owner.clone() };
                self.psql_call(&field, entity, owner.as_deref(), method, args, pos)?
            }
            InfraKind::HttpClient {
                notify: true,
                record,
                ..
            } if method == "send" || method == "send_recorded" => {
                let [kind, text] =
                    self.positional(args, 2, &format!("{}(kind, text)", method), pos)?[..]
                else {
                    unreachable!()
                };
                let (kty, kind) = self.expr(kind, Some(&Ty::Str))?;
                let kind = match kty {
                    Ty::Str => format!("&{}", kind),
                    Ty::Enum(_) => format!("{}.as_str()", kind),
                    other => {
                        return err(
                            pos,
                            format!(
                                "the kind is a String or an enum value, not {}",
                                other.name()
                            ),
                        );
                    }
                };
                let text = self.expect(text, &Ty::Str)?;
                self.need_try(pos, &format!("calling {}", infra))?;
                if !self.out.infras.iter().any(|i| i == infra) {
                    self.out.infras.push(infra.to_string());
                }
                if method == "send" {
                    return Ok((
                        Ty::Unit,
                        format!("let _ = {}.send({}, &{}).await", field, kind, text),
                    ));
                }
                let Some(record) = record.clone() else {
                    return err(
                        pos,
                        format!(
                            "'{}' records nothing: give its notifications a record infra, or use send()",
                            infra
                        ),
                    );
                };
                let entity = match self.p.infras.get(&record) {
                    Some(InfraKind::Psql { entity, .. }) => entity.clone(),
                    _ => return err(pos, format!("'{}' is not a psql infra", record)),
                };
                if !self.out.infras.iter().any(|i| i == &record) {
                    self.out.infras.push(record.clone());
                }
                return Ok((
                    Ty::Struct(entity),
                    format!(
                        "{}.send_recorded(&self.{}, user, {}, &{}).await?",
                        field,
                        record.to_snake_case(),
                        kind,
                        text
                    ),
                ));
            }
            InfraKind::HttpClient { ics, .. } => {
                if method == "is_configured" {
                    self.positional(args, 0, "is_configured()", pos)?;
                    if !self.out.infras.iter().any(|i| i == infra) {
                        self.out.infras.push(infra.to_string());
                    }
                    return Ok((Ty::Bool, format!("{}.is_configured()", field)));
                }
                if method == "occurrences" {
                    if !ics {
                        return err(
                            pos,
                            format!(
                                "occurrences() reads an ics_feed infra; '{}' is an http_client",
                                infra
                            ),
                        );
                    }
                    let [from, to] = self.positional(args, 2, "occurrences(from, to)", pos)?[..]
                    else {
                        unreachable!()
                    };
                    let from = bare(&self.expect(from, &Ty::DateTime)?);
                    let to = bare(&self.expect(to, &Ty::DateTime)?);
                    (
                        Ty::list(Ty::Struct("IcsOccurrence".into())),
                        format!("{}.occurrences({}, {}).await?", field, from, to),
                    )
                } else {
                    self.http_call(&field, method, args, expected, pos)?
                }
            }
            InfraKind::Other(kind) => {
                return err(
                    pos,
                    format!(
                        "logic can't use {} infras ('{}') yet: write this use case with code {{}}",
                        kind, infra
                    ),
                );
            }
        };
        self.need_try(pos, &format!("calling {}", infra))?;
        if !self.out.infras.iter().any(|i| i == infra) {
            self.out.infras.push(infra.to_string());
        }
        Ok(code)
    }

    fn psql_call(
        &mut self,
        field: &str,
        entity: &str,
        owner: Option<&str>,
        method: &str,
        args: &[Arg],
        pos: Pos,
    ) -> R<(Ty, String)> {
        let row = Ty::Struct(entity.to_string());
        let fields = self.p.fields(entity).or_else(|e| err(pos, e))?;
        let has_id = fields.iter().any(|(f, _)| f == "id");
        let not_found = format!("{}::NotFound", self.error);
        // The caller owns what it creates or updates.
        let stamp = match owner {
            Some(o) => {
                let optional = fields
                    .iter()
                    .any(|(f, t)| f == o && matches!(t, crate::parser::AllowedType::Option(_)));
                format!(
                    "trust_row.{} = {}; ",
                    o,
                    if optional { "Some(user)" } else { "user" }
                )
            }
            None => String::new(),
        };
        Ok(match method {
            "find" | "get" => {
                let [id] = self.positional(args, 1, &format!("{}(id)", method), pos)?[..] else {
                    unreachable!()
                };
                let id = bare(&self.expect(id, &Ty::Int)?);
                let call = match owner {
                    Some(o) => format!(
                        "{}.find_one_owned({}, {}, user).await?",
                        field,
                        id,
                        literal(o)
                    ),
                    None => format!("{}.find_one({}).await?", field, id),
                };
                if method == "find" {
                    (Ty::opt(row), call)
                } else {
                    (row, format!("{}.ok_or({})?", call, not_found))
                }
            }
            "query" | "all" => {
                let mut query = String::from("Query::new()");
                if let Some(o) = owner {
                    query.push_str(&format!(".eq({}, user)", literal(o)));
                }
                if method == "all" && args.iter().any(|a| a.name.is_none()) {
                    return err(
                        pos,
                        "all() takes no conditions: use query(field == value, ...)",
                    );
                }
                for arg in args {
                    query.push_str(&self.query_arg(entity, arg)?);
                }
                (Ty::list(row), format!("{}.query({}).await?", field, query))
            }
            "create" => {
                let [value] = self.positional(args, 1, &format!("create({})", entity), pos)?[..]
                else {
                    unreachable!()
                };
                let value = bare(&self.expect(value, &row)?);
                let code = if stamp.is_empty() {
                    format!("{}.save_one({}).await?", field, value)
                } else {
                    format!(
                        "{{ let mut trust_row = {}; {}{}.save_one(trust_row).await? }}",
                        value, stamp, field
                    )
                };
                (row, code)
            }
            "update" => {
                if !has_id {
                    return err(pos, format!("update() needs an 'id' field on {}", entity));
                }
                let [value] = self.positional(args, 1, &format!("update({})", entity), pos)?[..]
                else {
                    unreachable!()
                };
                let value = bare(&self.expect(value, &row)?);
                let code = match owner {
                    Some(o) => format!(
                        "{{ let mut trust_row = {}; {}{}.update_one_owned(trust_row.id, {}, user, trust_row).await?.ok_or({})? }}",
                        value,
                        stamp,
                        field,
                        literal(o),
                        not_found
                    ),
                    None => format!(
                        "{{ let trust_row = {}; {}.update_one(trust_row.id, trust_row).await? }}",
                        value, field
                    ),
                };
                (row, code)
            }
            "delete" => {
                let [id] = self.positional(args, 1, "delete(id)", pos)?[..] else {
                    unreachable!()
                };
                let id = bare(&self.expect(id, &Ty::Int)?);
                let code = match owner {
                    Some(o) => format!(
                        "{}.delete_owned({}, {}, user).await?.ok_or({})?",
                        field,
                        id,
                        literal(o),
                        not_found
                    ),
                    None => format!("{}.delete({}).await?", field, id),
                };
                (row, code)
            }
            other => {
                return err(
                    pos,
                    format!(
                        "a psql infra has find, get, query, all, create, update and delete — not '{}'",
                        other
                    ),
                );
            }
        })
    }

    /// One argument of `query(...)`: `field == value`, `field in list`,
    /// `field.contains(text)`, or `order_by: field`, `limit: n`, ...
    fn query_arg(&mut self, entity: &str, arg: &Arg) -> R<String> {
        let row = Ty::Struct(entity.to_string());
        let pos = arg.value.pos;
        let field_of = |this: &Self, e: &Expr| -> R<(String, Ty)> {
            match &e.kind {
                ExprKind::Var(f) => {
                    let ty = this.field_ty(&row, f, e.pos)?;
                    Ok((f.clone(), ty))
                }
                _ => err(e.pos, format!("expected a field of {}", entity)),
            }
        };
        if let Some(name) = &arg.name {
            return match name.as_str() {
                "order_by" | "order_by_desc" => {
                    let (f, _) = field_of(self, &arg.value)?;
                    Ok(format!(".{}({})", name, literal(&f)))
                }
                "limit" | "offset" => Ok(format!(
                    ".{}({})",
                    name,
                    bare(&self.expect(&arg.value, &Ty::Int)?)
                )),
                other => err(
                    pos,
                    format!(
                        "query() options are order_by, order_by_desc, limit and offset — not '{}'",
                        other
                    ),
                ),
            };
        }
        let filterable = |ty: &Ty, at: Pos| -> R<Ty> {
            let inner = match ty {
                Ty::Opt(inner) => (**inner).clone(),
                t => t.clone(),
            };
            if !inner.is_display() || inner == Ty::Json {
                return err(at, format!("{} fields can't be filtered on", ty.name()));
            }
            Ok(inner)
        };
        match &arg.value.kind {
            ExprKind::Binary(op, left, right) => {
                let (f, fty) = field_of(self, left)?;
                let inner = filterable(&fty, left.pos)?;
                let method = match op {
                    BinOp::Eq => "eq",
                    BinOp::Ne => "ne",
                    BinOp::Lt => "lt",
                    BinOp::Le => "lte",
                    BinOp::Gt => "gt",
                    BinOp::Ge => "gte",
                    BinOp::In => "is_in",
                    BinOp::NotIn => "not_in",
                    other => {
                        return err(pos, format!("'{}' isn't a query condition", other.symbol()));
                    }
                };
                let value = if matches!(op, BinOp::In | BinOp::NotIn) {
                    bare(&self.expect(right, &Ty::list(inner))?)
                } else {
                    if matches!(right.kind, ExprKind::None) {
                        return err(right.pos, "queries can't match 'none' yet");
                    }
                    bare(&self.expect(right, &inner)?)
                };
                Ok(format!(".{}({}, {})", method, literal(&f), value))
            }
            ExprKind::Method(recv, method, margs)
                if ["contains", "like", "ilike"].contains(&method.as_str()) =>
            {
                let (f, fty) = field_of(self, recv)?;
                if filterable(&fty, recv.pos)? != Ty::Str {
                    return err(recv.pos, format!("{}() filters String fields", method));
                }
                let [text] =
                    self.positional(margs, 1, &format!("{}.{}(text)", f, method), pos)?[..]
                else {
                    unreachable!()
                };
                let text = self.expect(text, &Ty::Str)?;
                Ok(format!(".{}({}, {})", method, literal(&f), text))
            }
            _ => err(
                pos,
                format!(
                    "a query condition looks like 'field == value' (field of {})",
                    entity
                ),
            ),
        }
    }

    fn http_call(
        &mut self,
        field: &str,
        method: &str,
        args: &[Arg],
        expected: Option<&Ty>,
        pos: Pos,
    ) -> R<(Ty, String)> {
        // The response decodes into what the context wants (`let r: T =`,
        // `return`, a struct field), JSON otherwise.
        let response = match expected {
            Some(t) if !matches!(t, Ty::Unit | Ty::Never | Ty::RawRequest) => t.clone(),
            _ => Ty::Json,
        };
        Ok(match method {
            "get" | "delete" => {
                let [path] = self.positional(args, 1, &format!("{}(path)", method), pos)?[..]
                else {
                    unreachable!()
                };
                let path = self.expect(path, &Ty::Str)?;
                (
                    response.clone(),
                    format!(
                        "{}.{}_json::<{}>(&{}).await?",
                        field,
                        method,
                        response.rust(),
                        path
                    ),
                )
            }
            "post" | "put" | "patch" => {
                let [path, body] =
                    self.positional(args, 2, &format!("{}(path, body)", method), pos)?[..]
                else {
                    unreachable!()
                };
                let path = self.expect(path, &Ty::Str)?;
                let (bty, body) = self.expr(body, None)?;
                if matches!(bty, Ty::Unit | Ty::Never) {
                    return err(pos, "the body doesn't produce a value");
                }
                (
                    response.clone(),
                    format!(
                        "{}.{}_json::<_, {}>(&{}, &{}).await?",
                        field,
                        method,
                        response.rust(),
                        path,
                        body
                    ),
                )
            }
            "get_paged" => {
                let [path, items, next] = self.positional(
                    args,
                    3,
                    "get_paged(path, items_field, next_token_field)",
                    pos,
                )?[..] else {
                    unreachable!()
                };
                let path = self.expect(path, &Ty::Str)?;
                let items = self.expect(items, &Ty::Str)?;
                let next = self.expect(next, &Ty::Str)?;
                let item = match &response {
                    Ty::List(item) => (**item).clone(),
                    _ => Ty::Json,
                };
                (
                    Ty::list(item.clone()),
                    format!(
                        "{}.get_json_paged::<{}>(&{}, &{}, &{}).await?",
                        field,
                        item.rust(),
                        path,
                        items,
                        next
                    ),
                )
            }
            "send" => {
                if args.len() != 2 && args.len() != 3 {
                    return err(
                        pos,
                        "expected send(\"METHOD\", path) or send(\"METHOD\", path, body)",
                    );
                }
                let verb = match &args[0].value.kind {
                    ExprKind::Str(parts) => match parts.as_slice() {
                        [StrPart::Text(v)]
                            if ["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&v.as_str()) =>
                        {
                            v.clone()
                        }
                        _ => {
                            return err(
                                args[0].value.pos,
                                "send() takes a literal \"GET\", \"POST\", \"PUT\", \"PATCH\" or \"DELETE\"",
                            );
                        }
                    },
                    _ => {
                        return err(
                            args[0].value.pos,
                            "send() takes a literal \"GET\", \"POST\", \"PUT\", \"PATCH\" or \"DELETE\"",
                        );
                    }
                };
                let path = self.expect(&args[1].value, &Ty::Str)?;
                let body = match args.get(2) {
                    Some(arg) => {
                        let (bty, code) = self.expr(&arg.value, None)?;
                        if matches!(bty, Ty::Unit | Ty::Never) {
                            return err(pos, "the body doesn't produce a value");
                        }
                        format!(".json(&{})", code)
                    }
                    None => String::new(),
                };
                (
                    Ty::Response,
                    format!(
                        "{{ let (trust_status, trust_text) = {f}.send_raw({f}.request(reqwest::Method::{v}, &{p}){b}).await?; (trust_status.as_u16() as i64, trust_text) }}",
                        f = field,
                        v = verb,
                        p = path,
                        b = body
                    ),
                )
            }
            "text" => {
                let [path] = self.positional(args, 1, "text(path)", pos)?[..] else {
                    unreachable!()
                };
                let path = self.expect(path, &Ty::Str)?;
                (Ty::Str, format!("{}.get_text(&{}).await?", field, path))
            }
            other => {
                return err(
                    pos,
                    format!(
                        "an http_client infra has get, post, put, patch, delete, text, get_paged, send and is_configured — not '{}'",
                        other
                    ),
                );
            }
        })
    }

    /// `x => body` over items of `item`: the Rust parameter and body.
    fn lambda(&mut self, arg: &Arg, item: &Ty, pos: Pos) -> R<(String, Ty, String)> {
        let ExprKind::Lambda(param, body) = &arg.value.kind else {
            return err(pos, "expected 'x => ...'");
        };
        self.scopes.push(Vec::new());
        self.lambda += 1;
        let result = (|| {
            let rust = self.declare(param, item.clone(), false, true, arg.value.pos)?;
            let (ty, code) = self.expr(body, None)?;
            Ok((rust, ty, bare(&code)))
        })();
        self.lambda -= 1;
        self.scopes.pop();
        result
    }

    fn value_method(
        &mut self,
        recv_expr: &Expr,
        name: &str,
        args: &[Arg],
        pos: Pos,
    ) -> R<(Ty, String)> {
        let (ty, recv) = self.recv(recv_expr)?;
        let sig = |s: &str| format!("{}.{}", ty.name(), s);
        if name == "to_string" && ty.is_display() {
            self.positional(args, 0, "to_string()", pos)?;
            return Ok((Ty::Str, format!("{}.to_string()", recv)));
        }
        let unknown = |ty: &Ty| -> R<(Ty, String)> {
            err(pos, format!("a {} has no method '{}'", ty.name(), name))
        };
        match &ty {
            Ty::Str => Ok(match name {
                "len" => (Ty::Int, format!("({}.chars().count() as i64)", recv)),
                "is_empty" => (Ty::Bool, format!("{}.is_empty()", recv)),
                "trim" => (Ty::Str, format!("{}.trim().to_string()", recv)),
                "lower" => (Ty::Str, format!("{}.to_lowercase()", recv)),
                "upper" => (Ty::Str, format!("{}.to_uppercase()", recv)),
                "to_int" => (
                    Ty::opt(Ty::Int),
                    format!("{}.trim().parse::<i64>().ok()", recv),
                ),
                "to_float" => (
                    Ty::opt(Ty::Float),
                    format!("{}.trim().parse::<f64>().ok()", recv),
                ),
                "contains" | "starts_with" | "ends_with" => {
                    let [text] =
                        self.positional(args, 1, &sig(&format!("{}(text)", name)), pos)?[..]
                    else {
                        unreachable!()
                    };
                    let text = self.expect(text, &Ty::Str)?;
                    (Ty::Bool, format!("{}.{}({}.as_str())", recv, name, text))
                }
                "take" | "skip" => {
                    let [n] = self.positional(args, 1, &sig(&format!("{}(n)", name)), pos)?[..]
                    else {
                        unreachable!()
                    };
                    let n = bare(&self.expect(n, &Ty::Int)?);
                    (
                        Ty::Str,
                        format!(
                            "{}.chars().{}(usize::try_from({}).unwrap_or(0)).collect::<String>()",
                            recv, name, n
                        ),
                    )
                }
                "split" => {
                    let [sep] = self.positional(args, 1, &sig("split(separator)"), pos)?[..] else {
                        unreachable!()
                    };
                    let sep = self.expect(sep, &Ty::Str)?;
                    (
                        Ty::list(Ty::Str),
                        format!(
                            "{}.split({}.as_str()).map(|s| s.to_string()).collect::<Vec<String>>()",
                            recv, sep
                        ),
                    )
                }
                "replace" => {
                    let [from, to] = self.positional(args, 2, &sig("replace(from, to)"), pos)?[..]
                    else {
                        unreachable!()
                    };
                    let from = self.expect(from, &Ty::Str)?;
                    let to = self.expect(to, &Ty::Str)?;
                    (
                        Ty::Str,
                        format!("{}.replace({}.as_str(), {}.as_str())", recv, from, to),
                    )
                }
                _ => return unknown(&ty),
            })
            .and_then(|r| self.no_args_check(args, &r, name, pos)),
            Ty::Int => Ok(match name {
                "to_float" => (Ty::Float, format!("({} as f64)", recv)),
                "abs" => (Ty::Int, format!("{}.wrapping_abs()", recv)),
                _ => return unknown(&ty),
            })
            .and_then(|r| self.no_args_check(args, &r, name, pos)),
            Ty::Float => Ok(match name {
                "round" | "floor" | "ceil" => (Ty::Int, format!("({}.{}() as i64)", recv, name)),
                "abs" => (Ty::Float, format!("{}.abs()", recv)),
                _ => return unknown(&ty),
            })
            .and_then(|r| self.no_args_check(args, &r, name, pos)),
            Ty::DateTime | Ty::Date => self.time_method(&ty, &recv, name, args, pos),
            Ty::Opt(_) => match name {
                "is_some" | "is_none" => {
                    self.positional(args, 0, &format!("{}()", name), pos)?;
                    Ok((Ty::Bool, format!("{}.{}()", recv, name)))
                }
                _ => err(
                    pos,
                    format!(
                        "this {} may be none: unwrap it first ('if let', '??')",
                        ty.name()
                    ),
                ),
            },
            Ty::Json => match name {
                "str" => Ok((
                    Ty::opt(Ty::Str),
                    format!("{}.as_str().map(|s| s.to_string())", recv),
                )),
                "int" => Ok((Ty::opt(Ty::Int), format!("{}.as_i64()", recv))),
                "float" => Ok((Ty::opt(Ty::Float), format!("{}.as_f64()", recv))),
                "bool" => Ok((Ty::opt(Ty::Bool), format!("{}.as_bool()", recv))),
                "is_null" => Ok((Ty::Bool, format!("{}.is_null()", recv))),
                "list" => Ok((
                    Ty::list(Ty::Json),
                    format!("{}.as_array().cloned().unwrap_or_default()", recv),
                )),
                "get" => {
                    let [key] = self.positional(args, 1, "get(key)", pos)?[..] else {
                        unreachable!()
                    };
                    let key = self.expect(key, &Ty::Str)?;
                    return Ok((Ty::Json, format!("{}[{}.as_str()].clone()", recv, key)));
                }
                _ => unknown(&ty),
            }
            .and_then(|r| self.no_args_check(args, &r, name, pos)),
            Ty::RawRequest if name == "header" => {
                let [header] = self.positional(args, 1, "header(name)", pos)?[..] else {
                    unreachable!()
                };
                let header = self.expect(header, &Ty::Str)?;
                Ok((
                    Ty::opt(Ty::Str),
                    format!("{}.headers.get(&{}.to_lowercase()).cloned()", recv, header),
                ))
            }
            Ty::Struct(from) if name.starts_with("to_") => {
                let target = self
                    .p
                    .maps
                    .iter()
                    .find(|(f, t)| f == from && format!("to_{}", t.to_snake_case()) == name)
                    .map(|(_, t)| t.clone());
                match target {
                    Some(to) => {
                        self.positional(args, 0, &format!("{}()", name), pos)?;
                        Ok((Ty::Struct(to), format!("{}.{}()", recv, name)))
                    }
                    None => unknown(&ty),
                }
            }
            Ty::Map(key, value) => {
                let (key, value) = ((**key).clone(), (**value).clone());
                self.map_method(recv_expr, &recv, (key, value), name, args, pos)
            }
            Ty::Tagged(enum_name) if name == "is" => {
                let [variant] =
                    self.positional(args, 1, &format!("is({}.<value>)", enum_name), pos)?[..]
                else {
                    unreachable!()
                };
                let ExprKind::Field(t, v) = &variant.kind else {
                    return err(variant.pos, format!("expected {}.<value>", enum_name));
                };
                if !matches!(&t.kind, ExprKind::Var(t) if t == enum_name)
                    || self.p.variant_fields(enum_name, v).is_none()
                {
                    return err(
                        variant.pos,
                        format!(
                            "expected one of {}: {}",
                            enum_name,
                            self.p.enum_variants(enum_name).join(", ")
                        ),
                    );
                }
                Ok((
                    Ty::Bool,
                    format!(
                        "matches!({}, {}::{} {{ .. }})",
                        recv,
                        enum_name,
                        variant_ident(v)
                    ),
                ))
            }
            Ty::Try(_) if name == "is_ok" || name == "is_error" => {
                self.positional(args, 0, &format!("{}()", name), pos)?;
                let method = if name == "is_ok" { "is_ok" } else { "is_err" };
                Ok((Ty::Bool, format!("{}.{}()", recv, method)))
            }
            Ty::Response if name == "json" => {
                self.positional(args, 0, "json()", pos)?;
                Ok((
                    Ty::Json,
                    format!(
                        "serde_json::from_str::<serde_json::Value>(&{}.1).unwrap_or_default()",
                        recv
                    ),
                ))
            }
            Ty::List(item) => self.list_method(recv_expr, &recv, item, name, args, pos),
            _ => unknown(&ty),
        }
    }

    /// Today's date: in the app's `config { tz }` when it has one.
    fn today(&self) -> String {
        if self.p.local_time {
            "crate::tz::today()".into()
        } else {
            "chrono::Utc::now().date_naive()".into()
        }
    }

    /// An instant seen in the app's time zone (UTC without `config { tz }`).
    fn in_app_tz(&self, instant: &str) -> String {
        if self.p.local_time {
            format!("crate::tz::local({})", instant)
        } else {
            instant.to_string()
        }
    }

    fn time_method(
        &mut self,
        ty: &Ty,
        recv: &str,
        name: &str,
        args: &[Arg],
        pos: Pos,
    ) -> R<(Ty, String)> {
        self.dep("chrono", "0.4");
        let date = *ty == Ty::Date;
        let unknown = || err(pos, format!("a {} has no method '{}'", ty.name(), name));
        match name {
            "add_days" | "add_hours" | "add_minutes" | "add_seconds" => {
                if date && name != "add_days" {
                    return unknown();
                }
                let [n] = self.positional(args, 1, &format!("{}(n)", name), pos)?[..] else {
                    unreachable!()
                };
                let n = bare(&self.expect(n, &Ty::Int)?);
                let unit = name.trim_start_matches("add_");
                Ok((
                    ty.clone(),
                    format!("({} + chrono::Duration::{}({}))", recv, unit, n),
                ))
            }
            "days_since" | "minutes_since" | "seconds_since" => {
                if date != (name == "days_since") {
                    return unknown();
                }
                let [other] = self.positional(args, 1, &format!("{}(other)", name), pos)?[..]
                else {
                    unreachable!()
                };
                let other = bare(&self.expect(other, ty)?);
                let unit = name.trim_end_matches("_since");
                Ok((Ty::Int, format!("({} - {}).num_{}()", recv, other, unit)))
            }
            "weekday" => {
                self.positional(args, 0, "weekday()", pos)?;
                let value = if date {
                    recv.to_string()
                } else {
                    self.in_app_tz(recv)
                };
                Ok((
                    Ty::Int,
                    format!(
                        "(chrono::Datelike::weekday(&{}).number_from_monday() as i64)",
                        value
                    ),
                ))
            }
            "day" | "month" | "year" => {
                self.positional(args, 0, &format!("{}()", name), pos)?;
                let value = if date {
                    recv.to_string()
                } else {
                    self.in_app_tz(recv)
                };
                Ok((
                    Ty::Int,
                    format!("(chrono::Datelike::{}(&{}) as i64)", name, value),
                ))
            }
            "hour" | "minute" if !date => {
                self.positional(args, 0, &format!("{}()", name), pos)?;
                Ok((
                    Ty::Int,
                    format!(
                        "(chrono::Timelike::{}(&{}) as i64)",
                        name,
                        self.in_app_tz(recv)
                    ),
                ))
            }
            "timestamp" if !date => {
                self.positional(args, 0, "timestamp()", pos)?;
                Ok((Ty::Int, format!("{}.timestamp()", recv)))
            }
            "format" => {
                let [pattern] = self.positional(args, 1, "format(pattern)", pos)?[..] else {
                    unreachable!()
                };
                if !self.p.locale {
                    return err(
                        pos,
                        "format() writes dates in the app's language: declare config { locale: \"fr\" }",
                    );
                }
                let pattern = self.expect(pattern, &Ty::Str)?;
                Ok((
                    Ty::Str,
                    format!("crate::fmt::fmt_date({}, &{})", recv, pattern),
                ))
            }
            "date" if !date => {
                self.positional(args, 0, "date()", pos)?;
                Ok((Ty::Date, format!("{}.date_naive()", self.in_app_tz(recv))))
            }
            _ => unknown(),
        }
    }

    fn map_method(
        &mut self,
        recv_expr: &Expr,
        recv: &str,
        (key, value): (Ty, Ty),
        name: &str,
        args: &[Arg],
        pos: Pos,
    ) -> R<(Ty, String)> {
        let map = Ty::Map(Box::new(key.clone()), Box::new(value.clone()));
        Ok(match name {
            "len" => {
                self.positional(args, 0, "len()", pos)?;
                (Ty::Int, format!("({}.len() as i64)", recv))
            }
            "is_empty" => {
                self.positional(args, 0, "is_empty()", pos)?;
                (Ty::Bool, format!("{}.is_empty()", recv))
            }
            "keys" | "values" => {
                self.positional(args, 0, &format!("{}()", name), pos)?;
                let ty = if name == "keys" { key } else { value };
                (
                    Ty::list(ty),
                    format!("{}.{}().cloned().collect::<Vec<_>>()", recv, name),
                )
            }
            "get" | "contains" => {
                let [k] = self.positional(args, 1, &format!("{}(key)", name), pos)?[..] else {
                    unreachable!()
                };
                let k = self.expect(k, &key)?;
                if name == "get" {
                    (Ty::opt(value), format!("{}.get(&{}).cloned()", recv, k))
                } else {
                    (Ty::Bool, format!("{}.contains_key(&{})", recv, k))
                }
            }
            "set" | "remove" => {
                let Some(place) = self.place(recv_expr)? else {
                    return err(pos, format!("{}() changes a 'var' Map", name));
                };
                self.check_mutable(&place, pos)?;
                if name == "set" {
                    let [k, v] = self.positional(args, 2, "set(key, value)", pos)?[..] else {
                        unreachable!()
                    };
                    let k = bare(&self.expect(k, &key)?);
                    let v = bare(&self.expect(v, &value)?);
                    (Ty::Unit, format!("{}.insert({}, {})", place.code, k, v))
                } else {
                    let [k] = self.positional(args, 1, "remove(key)", pos)?[..] else {
                        unreachable!()
                    };
                    let k = self.expect(k, &key)?;
                    (Ty::Unit, format!("{}.remove(&{})", place.code, k))
                }
            }
            _ => {
                return err(
                    pos,
                    format!(
                        "a {} has len, is_empty, keys, values, get, contains, set and remove — not '{}'",
                        map.name(),
                        name
                    ),
                );
            }
        })
    }

    fn no_args_check(
        &self,
        args: &[Arg],
        result: &(Ty, String),
        name: &str,
        pos: Pos,
    ) -> R<(Ty, String)> {
        let takes_args = [
            "contains",
            "starts_with",
            "ends_with",
            "split",
            "replace",
            "get",
            "take",
            "skip",
        ]
        .contains(&name);
        if !takes_args && !args.is_empty() {
            return err(pos, format!("{}() takes no arguments", name));
        }
        Ok(result.clone())
    }

    fn list_method(
        &mut self,
        recv_expr: &Expr,
        recv: &str,
        item: &Ty,
        name: &str,
        args: &[Arg],
        pos: Pos,
    ) -> R<(Ty, String)> {
        let item = item.clone();
        let list = Ty::list(item.clone());
        let none = |this: &Self| {
            this.positional(args, 0, &format!("{}()", name), pos)
                .map(|_| ())
        };
        Ok(match name {
            "len" => {
                none(self)?;
                (Ty::Int, format!("({}.len() as i64)", recv))
            }
            "is_empty" => {
                none(self)?;
                (Ty::Bool, format!("{}.is_empty()", recv))
            }
            "first" | "last" => {
                none(self)?;
                (Ty::opt(item), format!("{}.{}().cloned()", recv, name))
            }
            "reverse" => {
                none(self)?;
                (
                    list,
                    format!("{}.iter().rev().cloned().collect::<Vec<_>>()", recv),
                )
            }
            "sum" => {
                none(self)?;
                if !item.is_numeric() {
                    return err(pos, format!("sum() adds numbers, not {}", item.name()));
                }
                (
                    item.clone(),
                    format!("{}.iter().copied().sum::<{}>()", recv, item.rust()),
                )
            }
            "min" | "max" => {
                none(self)?;
                if !item.is_total() {
                    return err(
                        pos,
                        format!(
                            "{}() needs comparable items (Int, String, dates...), not {}",
                            name,
                            item.name()
                        ),
                    );
                }
                (
                    Ty::opt(item),
                    format!("{}.iter().{}().cloned()", recv, name),
                )
            }
            "get" => {
                let [index] = self.positional(args, 1, "get(index)", pos)?[..] else {
                    unreachable!()
                };
                let index = bare(&self.expect(index, &Ty::Int)?);
                (
                    Ty::opt(item),
                    format!(
                        "usize::try_from({}).ok().and_then(|trust_i| {}.get(trust_i)).cloned()",
                        index, recv
                    ),
                )
            }
            "take" | "skip" => {
                let [n] = self.positional(args, 1, &format!("{}(n)", name), pos)?[..] else {
                    unreachable!()
                };
                let n = bare(&self.expect(n, &Ty::Int)?);
                (
                    list,
                    format!(
                        "{}.iter().{}(usize::try_from({}).unwrap_or(0)).cloned().collect::<Vec<_>>()",
                        recv, name, n
                    ),
                )
            }
            "contains" => {
                let [value] = self.positional(args, 1, "contains(value)", pos)?[..] else {
                    unreachable!()
                };
                if !item.is_eq() {
                    return err(pos, format!("{} values can't be compared", item.name()));
                }
                let value = self.expect(value, &item)?;
                (Ty::Bool, format!("{}.contains(&{})", recv, value))
            }
            "join" => {
                if item != Ty::Str {
                    return err(pos, "join() glues a List<String>");
                }
                let [sep] = self.positional(args, 1, "join(separator)", pos)?[..] else {
                    unreachable!()
                };
                let sep = self.expect(sep, &Ty::Str)?;
                (Ty::Str, format!("{}.join({}.as_str())", recv, sep))
            }
            "push" => {
                let [value] = self.positional(args, 1, "push(value)", pos)?[..] else {
                    unreachable!()
                };
                let Some(place) = self.place(recv_expr)? else {
                    return err(pos, "push() adds to a 'var' list");
                };
                self.check_mutable(&place, pos)?;
                let value = self.expect(value, &item)?;
                (Ty::Unit, format!("{}.push({})", place.code, bare(&value)))
            }
            "filter_map" => {
                let [_] = self.positional(args, 1, "filter_map(x => optional)", pos)?[..] else {
                    unreachable!()
                };
                let (param, body_ty, body) = self.lambda(&args[0], &item, pos)?;
                let Ty::Opt(inner) = body_ty else {
                    return err(
                        args[0].value.pos,
                        format!(
                            "filter_map() keeps the set values of an Option, found {}",
                            body_ty.name()
                        ),
                    );
                };
                (
                    Ty::list(*inner),
                    format!(
                        "{}.iter().filter_map(|{}| {}).collect::<Vec<_>>()",
                        recv, param, body
                    ),
                )
            }
            "distinct" => {
                self.positional(args, 0, "distinct()", pos)?;
                if !item.is_eq() {
                    return err(pos, format!("{} values can't be compared", item.name()));
                }
                (
                    list,
                    format!(
                        "{{ let mut trust_seen: Vec<{t}> = Vec::new(); for trust_item in {r}.iter() {{ if !trust_seen.contains(trust_item) {{ trust_seen.push(trust_item.clone()); }} }} trust_seen }}",
                        t = item.rust(),
                        r = recv
                    ),
                )
            }
            "group_by" | "to_map" => {
                let [_] = self.positional(args, 1, &format!("{}(x => key)", name), pos)?[..] else {
                    unreachable!()
                };
                let (param, key_ty, key) = self.lambda(&args[0], &item, pos)?;
                if !map_key(&key_ty) {
                    return err(
                        args[0].value.pos,
                        format!(
                            "a Map key is an Int, String, Bool, Date, DateTime or Uuid, not {}",
                            key_ty.name()
                        ),
                    );
                }
                let value = if name == "group_by" {
                    list.clone()
                } else {
                    item.clone()
                };
                let map = Ty::Map(Box::new(key_ty.clone()), Box::new(value));
                let insert = if name == "group_by" {
                    "trust_map.entry(trust_key({p})).or_default().push({p}.clone());"
                } else {
                    "trust_map.insert(trust_key({p}), {p}.clone());"
                }
                .replace("{p}", "trust_item");
                (
                    map.clone(),
                    format!(
                        "{{ let trust_key = |{p}: &{t}| -> {k} {{ {b} }}; let mut trust_map: {m} = Default::default(); for trust_item in {r}.iter() {{ {i} }} trust_map }}",
                        p = param,
                        t = item.rust(),
                        k = key_ty.rust(),
                        b = key,
                        m = map.rust(),
                        r = recv,
                        i = insert
                    ),
                )
            }
            "map" | "filter" | "any" | "all" | "find" | "count" | "sort_by" | "sort_by_desc" => {
                let [_] = self.positional(args, 1, &format!("{}(x => ...)", name), pos)?[..] else {
                    unreachable!()
                };
                let (param, body_ty, body) = self.lambda(&args[0], &item, pos)?;
                let predicate = |this: &Self| -> R<()> {
                    if body_ty != Ty::Bool {
                        return err(
                            args[0].value.pos,
                            format!(
                                "{}() needs a Bool condition, found {}",
                                name,
                                body_ty.name()
                            ),
                        );
                    }
                    let _ = this;
                    Ok(())
                };
                match name {
                    "map" => {
                        if matches!(body_ty, Ty::Unit | Ty::Never) {
                            return err(pos, "map() needs a value");
                        }
                        (
                            Ty::list(body_ty),
                            format!(
                                "{}.iter().map(|{}| {}).collect::<Vec<_>>()",
                                recv, param, body
                            ),
                        )
                    }
                    "any" | "all" => {
                        predicate(self)?;
                        (
                            Ty::Bool,
                            format!("{}.iter().{}(|{}| {})", recv, name, param, body),
                        )
                    }
                    "filter" => {
                        predicate(self)?;
                        (
                            list,
                            format!(
                                "{}.iter().filter(|{p}| {{ let {p} = *{p}; {b} }}).cloned().collect::<Vec<_>>()",
                                recv,
                                p = param,
                                b = body
                            ),
                        )
                    }
                    "find" => {
                        predicate(self)?;
                        (
                            Ty::opt(item),
                            format!(
                                "{}.iter().find(|{p}| {{ let {p} = *{p}; {b} }}).cloned()",
                                recv,
                                p = param,
                                b = body
                            ),
                        )
                    }
                    "count" => {
                        predicate(self)?;
                        (
                            Ty::Int,
                            format!(
                                "({}.iter().filter(|{p}| {{ let {p} = *{p}; {b} }}).count() as i64)",
                                recv,
                                p = param,
                                b = body
                            ),
                        )
                    }
                    _ => {
                        if !body_ty.is_ord() {
                            return err(
                                args[0].value.pos,
                                format!(
                                    "{}() needs a sortable key (Int, Float, String, dates), found {}",
                                    name,
                                    body_ty.name()
                                ),
                            );
                        }
                        let (a, b) = if name == "sort_by" {
                            ("trust_a", "trust_b")
                        } else {
                            ("trust_b", "trust_a")
                        };
                        (
                            list,
                            format!(
                                "{{ let mut trust_sorted = {}.clone(); trust_sorted.sort_by(|trust_a, trust_b| {{ let trust_key = |{}: &{}| -> {} {{ {} }}; trust_key({}).partial_cmp(&trust_key({})).unwrap_or(std::cmp::Ordering::Equal) }}); trust_sorted }}",
                                recv,
                                param,
                                item.rust(),
                                body_ty.rust(),
                                body,
                                a,
                                b
                            ),
                        )
                    }
                }
            }
            _ => return err(pos, format!("a List has no method '{}'", name)),
        })
    }
}
