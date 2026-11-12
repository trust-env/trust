//! `config { tz, locale }`: the crate's `trust_time.rs` — `crate::tz::*`
//! (local time in the app's zone) and `crate::fmt::*` (dates and plurals in
//! the app's language).

use std::path::Path;

use tera::Context;

use crate::generator::{CodeGenContext, rust_code::render::render};

/// Writes `src/trust_time.rs` when the project sets a time zone or a
/// locale; returns whether it did.
pub(super) fn write_time_module(context: &mut CodeGenContext, src_path: &Path) -> Result<bool, String> {
    if context.app_tz.is_none() && context.locale.is_none() {
        return Ok(false);
    }
    let mut ctx = Context::new();
    ctx.insert("tz", context.app_tz.as_deref().unwrap_or("UTC"));
    ctx.insert("locale", &context.locale);
    std::fs::write(src_path.join("trust_time.rs"), render("file/time/time.rs.tera", &ctx))
        .map_err(|e| format!("Failed to write the time module: {}", e))?;
    context.require_version_dependency_with_features("chrono", "0.4", vec!["serde"]);
    context.require_version_dependency("chrono-tz", "0.10");
    Ok(true)
}
