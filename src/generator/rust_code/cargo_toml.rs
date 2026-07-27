use std::{collections::HashMap, fs::File, io::Write, path::PathBuf};

use crate::generator::rust_code::render::render;
use serde::Serialize;
use tera::Context;

#[derive(Debug, Clone)]
pub enum Dependency {
    Git(String),
    Version(String),
    VersionWithFeatures(String, Vec<String>),
}

#[derive(Serialize)]
struct DependencyView {
    name: String,
    kind: &'static str,
    value: String,
    features_joined: String,
}

fn dependency_views(dependencies: &HashMap<String, Dependency>) -> Vec<DependencyView> {
    let mut names: Vec<&String> = dependencies.keys().collect();
    names.sort();

    names
        .into_iter()
        .map(|name| {
            let (kind, value, features_joined) = match &dependencies[name] {
                Dependency::Git(url) => ("git", url.clone(), String::new()),
                Dependency::Version(version) => ("version", version.clone(), String::new()),
                Dependency::VersionWithFeatures(version, features) => (
                    "version_with_features",
                    version.clone(),
                    features.iter().map(|f| format!("\"{}\"", f)).collect::<Vec<_>>().join(", "),
                ),
            };
            DependencyView {
                name: name.clone(),
                kind,
                value,
                features_joined,
            }
        })
        .collect()
}

pub struct CargoToml {
    file: File,
    package_name: String,
    edition: String,
}

impl CargoToml {
    pub fn new(path: PathBuf, package_name: String) -> Result<Self, String> {
        Ok(CargoToml {
            file: File::create(&path)
                .map_err(|e| format!("Failed to create Cargo.toml: {}", e))?,
            package_name,
            edition: "2024".to_string(),
        })
    }

    pub fn write(
        &mut self,
        dependencies: &HashMap<String, Dependency>,
        build_dependencies: &HashMap<String, Dependency>,
    ) -> Result<(), String> {
        let mut context = Context::new();
        context.insert("package_name", &self.package_name);
        context.insert("edition", &self.edition);
        context.insert("dependencies", &dependency_views(dependencies));
        context.insert("build_dependencies", &dependency_views(build_dependencies));
        let content = render("cargo_toml.toml.tera", &context);

        self.file
            .write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write Cargo.toml: {}", e))
    }
}

/// The workspace-root `Cargo.toml` written once per compile, listing every
/// named `app`'s own crate as a member — a virtual manifest (no
/// `[package]`), so `cargo build --manifest-path <root>/Cargo.toml` builds
/// every app's binary in one shot.
pub struct WorkspaceCargoToml {
    file: File,
}

impl WorkspaceCargoToml {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        Ok(WorkspaceCargoToml {
            file: File::create(&path)
                .map_err(|e| format!("Failed to create workspace Cargo.toml: {}", e))?,
        })
    }

    pub fn write(&mut self, members: &[String]) -> Result<(), String> {
        let mut context = Context::new();
        context.insert("members", members);
        let content = render("workspace_cargo_toml.toml.tera", &context);

        self.file
            .write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write workspace Cargo.toml: {}", e))
    }
}
