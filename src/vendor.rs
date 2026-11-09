//! `--vendor=<dir>`: makes a generated project build without network or
//! SSH access to the private git dependencies it pulls in (`common`,
//! `auth`, `user`, ...).
//!
//! Every git dependency found in the generated manifests whose crate has a
//! local copy under `<dir>/<name>` is copied into `<out>/vendor/<name>`
//! (recursively: a vendored crate's own git dependencies are vendored too),
//! and the root manifest gets one `[patch."<git url>"]` entry per crate.
//! Cargo applies `[patch]` to transitive dependencies as well, so `auth`'s
//! own `common = { git = ... }` resolves to the vendored copy too. The
//! result is a self-contained `out/` that a Docker build can copy as is.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Directory names never copied into `out/vendor/`.
const SKIPPED_DIRS: &[&str] = &["target", ".git", "out"];

/// One `name = { git = "url" ... }` line of a Cargo manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDependency {
    pub name: String,
    pub url: String,
}

/// Every `name = { git = "..." }` dependency of a manifest. Only the inline
/// table form is recognised — the form Trust generates and the BH-World
/// crates use.
pub fn git_dependencies(manifest: &str) -> Vec<GitDependency> {
    manifest
        .lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once('=')?;
            let name = name.trim();
            if name.is_empty() || name.starts_with('[') || name.starts_with('#') {
                return None;
            }
            let (_, after_git) = rest.split_once("git")?;
            let after_eq = after_git.trim_start().strip_prefix('=')?;
            let url = after_eq.trim_start().strip_prefix('"')?.split('"').next()?;
            Some(GitDependency {
                name: name.to_string(),
                url: url.to_string(),
            })
        })
        .collect()
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("Failed to create {}: {}", to.display(), e))?;
    let entries = fs::read_dir(from).map_err(|e| format!("Failed to read {}: {}", from.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read {}: {}", from.display(), e))?;
        let path = entry.path();
        let name = entry.file_name();
        let target = to.join(&name);
        if path.is_dir() {
            if SKIPPED_DIRS.iter().any(|s| name == *s) {
                continue;
            }
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target)
                .map_err(|e| format!("Failed to copy {}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

/// The manifests of a generated project: the root one plus each workspace
/// member's.
fn generated_manifests(out: &Path) -> Vec<PathBuf> {
    let mut manifests = vec![out.join("Cargo.toml")];
    if let Ok(entries) = fs::read_dir(out) {
        let mut members: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n != "vendor"))
            .map(|p| p.join("Cargo.toml"))
            .filter(|p| p.is_file())
            .collect();
        members.sort();
        manifests.extend(members);
    }
    manifests
}

/// Copies every git dependency of the project at `out` that has a local
/// copy in `vendor_dir` into `out/vendor/`, and patches the root manifest
/// to use those copies. Returns the vendored crate names, sorted.
pub fn vendor_dependencies(out: &Path, vendor_dir: &Path) -> Result<Vec<String>, String> {
    if !vendor_dir.is_dir() {
        return Err(format!("--vendor directory '{}' does not exist", vendor_dir.display()));
    }

    let mut pending: Vec<GitDependency> = Vec::new();
    for manifest in generated_manifests(out) {
        if let Ok(content) = fs::read_to_string(&manifest) {
            pending.extend(git_dependencies(&content));
        }
    }

    // name -> git url, in a stable order for the generated [patch] blocks.
    let mut vendored: BTreeMap<String, String> = BTreeMap::new();
    let mut missing: Vec<String> = Vec::new();
    while let Some(dep) = pending.pop() {
        if vendored.contains_key(&dep.name) || missing.contains(&dep.name) {
            continue;
        }
        let source = vendor_dir.join(&dep.name);
        if !source.join("Cargo.toml").is_file() {
            missing.push(dep.name);
            continue;
        }
        let target = out.join("vendor").join(&dep.name);
        copy_dir(&source, &target)?;
        let manifest = fs::read_to_string(target.join("Cargo.toml"))
            .map_err(|e| format!("Failed to read vendored {} manifest: {}", dep.name, e))?;
        pending.extend(git_dependencies(&manifest));
        vendored.insert(dep.name, dep.url);
    }

    if !missing.is_empty() {
        missing.sort();
        return Err(format!(
            "--vendor: no local copy of {} in '{}' (expected <dir>/<crate>/Cargo.toml)",
            missing.join(", "),
            vendor_dir.display()
        ));
    }

    if vendored.is_empty() {
        return Ok(Vec::new());
    }

    let root_manifest = out.join("Cargo.toml");
    let mut content = fs::read_to_string(&root_manifest)
        .map_err(|e| format!("Failed to read {}: {}", root_manifest.display(), e))?;
    // Vendored crates live inside the workspace directory; keep them out
    // of the workspace itself (they are plain path dependencies).
    if content.contains("[workspace]") && !content.contains("exclude = [\"vendor\"]") {
        content = content.replacen("[workspace]\n", "[workspace]\nexclude = [\"vendor\"]\n", 1);
    }
    let mut by_url: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, url) in &vendored {
        by_url.entry(url.as_str()).or_default().push(name.as_str());
    }
    for (url, names) in by_url {
        content.push_str(&format!("\n[patch.\"{}\"]\n", url));
        for name in names {
            content.push_str(&format!("{} = {{ path = \"vendor/{}\" }}\n", name, name));
        }
    }
    fs::write(&root_manifest, content)
        .map_err(|e| format!("Failed to write {}: {}", root_manifest.display(), e))?;

    Ok(vendored.into_keys().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_dependencies_reads_inline_git_tables_only() {
        let manifest = r#"[dependencies]
common = { git = "ssh://git@github.com/BH-World/common.git" }
auth = { git = "ssh://git@github.com/BH-World/auth.git", branch = "main" }
serde = { version = "1", features = ["derive"] }
tokio = "1"
"#;
        assert_eq!(
            git_dependencies(manifest),
            vec![
                GitDependency {
                    name: "common".into(),
                    url: "ssh://git@github.com/BH-World/common.git".into()
                },
                GitDependency {
                    name: "auth".into(),
                    url: "ssh://git@github.com/BH-World/auth.git".into()
                },
            ]
        );
    }

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn vendors_transitive_git_dependencies_and_patches_the_root_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        let local = tmp.path().join("mod");
        write(
            &out.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\nmembers = [\n    \"api\",\n]\n",
        );
        write(
            &out.join("api/Cargo.toml"),
            "[dependencies]\nauth = { git = \"ssh://x/auth.git\" }\n",
        );
        write(
            &local.join("auth/Cargo.toml"),
            "[dependencies]\ncommon = { git = \"ssh://x/common.git\" }\n",
        );
        write(&local.join("auth/src/lib.rs"), "");
        write(&local.join("auth/target/big"), "skip me");
        write(&local.join("common/Cargo.toml"), "[dependencies]\n");

        let vendored = vendor_dependencies(&out, &local).unwrap();

        assert_eq!(vendored, vec!["auth".to_string(), "common".to_string()]);
        assert!(out.join("vendor/auth/src/lib.rs").is_file());
        assert!(!out.join("vendor/auth/target").exists());
        let root = fs::read_to_string(out.join("Cargo.toml")).unwrap();
        assert!(root.contains("exclude = [\"vendor\"]"), "{}", root);
        assert!(root.contains("[patch.\"ssh://x/auth.git\"]\nauth = { path = \"vendor/auth\" }"));
        assert!(root.contains("[patch.\"ssh://x/common.git\"]\ncommon = { path = \"vendor/common\" }"));
    }

    #[test]
    fn missing_local_copy_is_a_clear_error() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("out");
        write(
            &out.join("Cargo.toml"),
            "[dependencies]\ncommon = { git = \"ssh://x/common.git\" }\n",
        );
        fs::create_dir_all(tmp.path().join("mod")).unwrap();
        let err = vendor_dependencies(&out, &tmp.path().join("mod")).unwrap_err();
        assert!(err.contains("no local copy of common"), "{}", err);
    }
}
