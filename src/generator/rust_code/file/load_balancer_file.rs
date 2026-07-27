use std::{fs::File, io::Write, path::PathBuf};

/// Writes the adapter-agnostic round-robin reverse proxy used by every
/// `load_balancer {}` in the `.tr` file: it forwards each request to the
/// next backend address in the list and streams the response back
/// untouched. One copy is written per generated project (not per
/// load_balancer), since it doesn't know or care what's behind each target.
pub struct LoadBalancerFile {
    file: File,
}

impl LoadBalancerFile {
    pub fn new(folder: PathBuf) -> Result<Self, String> {
        Ok(LoadBalancerFile {
            file: File::create(folder.join("load_balancer.rs"))
                .map_err(|e| format!("Failed to create load balancer file: {}", e))?,
        })
    }

    pub fn write(&mut self) -> Result<(), String> {
        // Fully static (no per-project variables), so this is a plain
        // bundled file rather than a Tera template.
        let code = include_str!("../../../../templates/file/load_balancer.rs");

        self.file
            .write_all(code.as_bytes())
            .map_err(|e| format!("Failed to write load balancer file: {}", e))
    }
}
