use std::sync::{LazyLock, RwLock};

use tera::{Context, Tera};

include!(concat!(env!("OUT_DIR"), "/templates.rs"));

/// Every generated-code template, embedded into the binary at compile time
/// (`include_str!`) so there's no runtime dependency on a `templates/`
/// directory being present next to the executable. Rust resolves all the
/// *data* (what a use case's input type is, which infra a call targets,
/// ...); these templates only ever decide the text *layout* of that data.
///
/// Wrapped in an `RwLock` (rather than a plain `Tera`) so that plugin-
/// supplied templates can be added at runtime, after this static's one-time
/// lazy init, via `register_plugin_templates` — every bundled template
/// above is still resolved purely at compile time.
static TEMPLATES: LazyLock<RwLock<Tera>> = LazyLock::new(|| {
    let mut tera = Tera::default();
    tera.add_raw_templates(BUNDLED_TEMPLATES.iter().copied())
    .expect("all bundled templates should be valid Tera syntax");
    RwLock::new(tera)
});

/// Renders `name` (a key registered above) against `context`, panicking with
/// the template name and Tera's own diagnostic on failure — a bundled
/// template failing to render is always a bug in this compiler, never
/// user-facing input.
pub fn render(name: &str, context: &Context) -> String {
    TEMPLATES
        .read()
        .unwrap_or_else(|e| panic!("template registry lock poisoned: {e}"))
        .render(name, context)
        .unwrap_or_else(|e| panic!("template '{}' failed to render: {}", name, e))
}

/// Same as `render`, but returns a `Result` instead of panicking. Used
/// exclusively by the plugin codegen path (`generator::plugin`), since a
/// plugin-supplied template failing to render is never a bug in this
/// compiler the way a bundled template failing would be.
pub fn try_render(name: &str, context: &Context) -> Result<String, String> {
    TEMPLATES
        .read()
        .map_err(|e| format!("template registry lock poisoned: {e}"))?
        .render(name, context)
        .map_err(|e| format!("template '{}' failed to render: {}", name, e))
}

/// Registers plugin-supplied `(name, content)` template pairs into the
/// shared registry, under whatever name the caller passes (callers should
/// namespace plugin template names, e.g. `plugin/<plugin_name>/<name>`, to
/// avoid colliding with a bundled template or another plugin's).
pub fn register_plugin_templates(templates: &[(String, String)]) -> Result<(), String> {
    let mut tera = TEMPLATES.write().map_err(|e| format!("template registry lock poisoned: {e}"))?;
    for (name, content) in templates {
        tera.add_raw_template(name, content)
            .map_err(|e| format!("plugin template '{}' is not valid Tera syntax: {}", name, e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_template_is_registered_and_parses() {
        // Building `TEMPLATES` parses every bundled template, so a syntax
        // error in any of them fails here with Tera's diagnostic.
        let tera = TEMPLATES.read().unwrap();
        let names: Vec<&str> = tera.get_template_names().collect();
        assert_eq!(names.len(), BUNDLED_TEMPLATES.len());
        for (name, _) in BUNDLED_TEMPLATES {
            assert!(names.contains(name), "{} not registered", name);
        }
        assert!(names.contains(&"types.rs.tera"));
        assert!(names.contains(&"file/psql_storage/storage.rs.tera"));
    }
}
