use std::{fs::File, io::Write, path::PathBuf};

use crate::generator::rust_code::render::render;
use tera::Context;

pub struct ModFile {
    file: File,
    mods: Vec<String>,
}

impl ModFile {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        Ok(ModFile {
            file: File::create(&path).map_err(|e| format!("Failed to create file: {}", e))?,
            mods: Vec::new(),
        })
    }

    pub fn add_mod(&mut self, module: String) {
        self.mods.push(module);
    }

    pub fn add_mods(&mut self, modules: Vec<String>) {
        self.mods.extend(modules);
    }

    pub fn with_mods(mut self, modules: Vec<String>) -> Self {
        self.add_mods(modules);
        self
    }

    pub fn write(&mut self) -> Result<(), String> {
        let mut context = Context::new();
        context.insert("mods", &self.mods);
        let content = render("file/mod_file.rs.tera", &context);

        self.file
            .write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write to file: {}", e))?;
        Ok(())
    }
}
