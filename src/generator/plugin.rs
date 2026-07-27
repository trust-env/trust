use std::path::Path;

use tera::Context;

use crate::{
    generator::{CodeGen, CodeGenContext, rust_code::render::try_render},
    parser::PluginAst,
    plugin::LoadedPlugin,
};

impl CodeGen {
    /// Hands a `Ast::Plugin` construct off to the plugin that registered its
    /// keyword: the plugin parses its own raw body text, decides what to
    /// generate (a template name + context + output file name), and the
    /// result is rendered through the same Tera registry every built-in
    /// writer uses (via `try_render`, not the panicking `render`, since a
    /// plugin-sourced template failing is never a bug in this compiler).
    pub(super) fn generate_plugin(
        &mut self,
        context: &mut CodeGenContext,
        plugin_ast: &PluginAst,
        loaded_plugins: &[LoadedPlugin],
        plugins_path: &Path,
        dev_mode: bool,
    ) -> Result<(), String> {
        let plugin = loaded_plugins
            .iter()
            .find(|p| p.keyword == plugin_ast.plugin_name)
            .ok_or_else(|| format!("no loaded plugin registered for construct '{}'", plugin_ast.plugin_name))?;

        let node = plugin.parse(&plugin_ast.raw_body)?;
        let host_context = serde_json::json!({ "plugin_name": plugin.name, "dev_mode": dev_mode });
        let result = plugin.codegen(&node, &host_context)?;

        let namespaced_template = format!("plugin/{}/{}", plugin.name, result.template_name);
        let tera_context = Context::from_serialize(&result.context)
            .map_err(|e| format!("plugin '{}' returned a non-object template context: {}", plugin.name, e))?;
        let rendered = try_render(&namespaced_template, &tera_context)?;

        std::fs::create_dir_all(plugins_path).map_err(|e| format!("failed to create plugins directory: {}", e))?;
        std::fs::write(plugins_path.join(&result.output_file), rendered)
            .map_err(|e| format!("failed to write plugin output file '{}': {}", result.output_file, e))?;

        let module = result.output_file.strip_suffix(".rs").unwrap_or(&result.output_file).to_string();
        context.register_plugin_module(module);
        Ok(())
    }
}
