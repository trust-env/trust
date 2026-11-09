//! Small, shared pieces of generated Rust (where-clauses, parameter lists,
//! `mod` declarations), each rendered from `templates/fragments/*.tera` so
//! no generator assembles Rust syntax with `format!`.

use serde::Serialize;
use tera::Context;

use crate::generator::rust_code::render::render;

#[derive(Serialize)]
struct BoundView<'a> {
    param: &'a str,
    traits: &'a [String],
}

#[derive(Serialize)]
struct ParamView<'a> {
    name: &'a str,
    ty: &'a str,
}

/// One `    T: A + B,` line per `(param, traits)` pair — the body of a
/// `where` clause, without the `where` keyword.
pub fn where_clause<P: AsRef<str>>(bounds: &[(P, Vec<String>)]) -> String {
    let bounds: Vec<BoundView> = bounds
        .iter()
        .map(|(p, traits)| BoundView { param: p.as_ref(), traits })
        .collect();
    let mut context = Context::new();
    context.insert("bounds", &bounds);
    render("fragments/where_clause.rs.tera", &context)
}

/// `a: A, b: B` — a function's parameter list, without the parentheses.
pub fn params<N: AsRef<str>, T: AsRef<str>>(params: &[(N, T)]) -> String {
    let params: Vec<ParamView> = params
        .iter()
        .map(|(n, t)| ParamView { name: n.as_ref(), ty: t.as_ref() })
        .collect();
    let mut context = Context::new();
    context.insert("params", &params);
    render("fragments/params.rs.tera", &context)
}

/// One `mod name;` line per module name.
pub fn mod_decls<M: AsRef<str>>(mods: &[M]) -> String {
    let mods: Vec<&str> = mods.iter().map(AsRef::as_ref).collect();
    let mut context = Context::new();
    context.insert("mods", &mods);
    render("fragments/mod_decls.rs.tera", &context)
}

/// `Trait + Send + Sync + 'static` — the bound every presenter puts on the
/// use cases and storages it holds, as a trait list for `where_clause`.
pub fn shared_bound(primary: &str) -> Vec<String> {
    vec![primary.to_string(), "Send".to_string(), "Sync".to_string(), "'static".to_string()]
}

/// `<A, B>` — a generic parameter list, or nothing when there are none.
pub fn type_params<P: AsRef<str>>(params: &[P]) -> String {
    let params: Vec<&str> = params.iter().map(AsRef::as_ref).collect();
    let mut context = Context::new();
    context.insert("params", &params);
    render("fragments/type_params.rs.tera", &context)
}

/// A full `where` block (keyword + one line per bound), or nothing when
/// there are no bounds.
pub fn where_block<P: AsRef<str>>(bounds: &[(P, Vec<String>)]) -> String {
    let bounds: Vec<BoundView> = bounds
        .iter()
        .map(|(p, traits)| BoundView { param: p.as_ref(), traits })
        .collect();
    let mut context = Context::new();
    context.insert("bounds", &bounds);
    render("fragments/where_block.rs.tera", &context)
}

/// A method body: each line indented to sit inside `async fn execute`,
/// empty lines left empty, joined without a trailing newline.
pub fn indented_body<L: AsRef<str>>(lines: &[L]) -> String {
    let lines: Vec<&str> = lines.iter().map(AsRef::as_ref).collect();
    let mut context = Context::new();
    context.insert("lines", &lines);
    render("fragments/indented_body.rs.tera", &context)
}
