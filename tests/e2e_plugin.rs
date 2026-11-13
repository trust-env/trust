//! End-to-end proof that the `--plugin` mechanism works for real: builds the
//! `note_plugin` example as an actual `.so`, runs the real compiled `trust`
//! binary against it via `dlopen` (not an in-process library call), and
//! asserts both the generated output and the generated *project* (plugin
//! output included) are correct — the strongest possible check, since it
//! exercises every link in the chain: dlopen -> keyword recognition ->
//! raw-body hand-off -> plugin parse -> plugin codegen -> namespaced
//! template render -> file written into the generated project -> generated
//! project still compiles.

use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Offline builds by default; CI sets `CARGO_NET_OFFLINE=false` so a fresh
/// runner can download crates.io dependencies.
fn cargo_net_offline() -> String {
    std::env::var("CARGO_NET_OFFLINE").unwrap_or_else(|_| "true".to_string())
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Builds the `note_plugin` example crate as a real `cdylib` and returns the
/// path to the resulting `.so`.
fn build_note_plugin() -> PathBuf {
    let manifest_path = repo_root().join("examples/plugins/note_plugin/Cargo.toml");
    let target_dir = repo_root().join("target/e2e-plugin");

    let output = Command::new("cargo")
        .args(["build", "--manifest-path"])
        .env("CARGO_NET_OFFLINE", cargo_net_offline())
        .arg(&manifest_path)
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("failed to spawn cargo build for note_plugin");
    assert_success(&output, "building note_plugin");

    let so_path = target_dir.join("debug").join("libnote_plugin.so");
    assert!(so_path.exists(), "expected {:?} to exist after building note_plugin", so_path);
    so_path
}

#[test]
fn note_plugin_generates_a_file_and_the_project_still_compiles() {
    let so_path = build_note_plugin();

    let tempdir = TempDir::new().expect("failed to create tempdir");
    let fixture_path = tempdir.path().join("fixture.tr");
    std::fs::write(
        &fixture_path,
        r#"type Greeting = String

note {
    message: "hello from a plugin"
}
"#,
    )
    .expect("failed to write fixture");

    let cli_output = Command::new(env!("CARGO_BIN_EXE_trust"))
        .arg(format!("--plugin={}", so_path.display()))
        .arg(&fixture_path)
        .current_dir(tempdir.path())
        .output()
        .expect("failed to spawn the trust CLI");
    assert_success(&cli_output, "trust CLI run with --plugin");

    let generated_root = tempdir.path().join("out");

    // The plugin's own output file, rendered through its own template.
    let note_file = generated_root.join("src/plugins/note.rs");
    let note_content = std::fs::read_to_string(&note_file).unwrap_or_else(|e| panic!("failed to read {:?}: {}", note_file, e));
    assert!(
        note_content.contains(r#"pub const NOTE: &str = "hello from a plugin";"#),
        "unexpected note.rs content: {}",
        note_content
    );

    // The generated project actually wires the plugin's module in.
    let plugins_mod_path = generated_root.join("src/plugins.rs");
    let plugins_mod = std::fs::read_to_string(&plugins_mod_path).expect("src/plugins.rs should exist");
    assert!(plugins_mod.contains("mod note;"), "unexpected src/plugins.rs content: {}", plugins_mod);

    let lib_rs_path = generated_root.join("src/lib.rs");
    let lib_rs = std::fs::read_to_string(&lib_rs_path).expect("src/lib.rs should exist");
    assert!(lib_rs.contains("mod plugins;"), "unexpected src/lib.rs content: {}", lib_rs);

    // Strongest possible proof: the whole generated project, plugin output
    // included, still compiles as a real Rust crate.
    let build_output = Command::new("cargo")
        .args(["build", "--manifest-path"])
        .env("CARGO_NET_OFFLINE", cargo_net_offline())
        .arg(generated_root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", repo_root().join("target/e2e-tests"))
        .output()
        .expect("failed to spawn cargo build for the generated project");
    assert_success(&build_output, "cargo build of the generated project");
}

#[test]
fn an_unregistered_plugin_keyword_is_rejected_without_a_matching_plugin() {
    // No --plugin flag at all: "note" is just an ordinary, unrecognized
    // top-level identifier, and must be rejected the same way any other
    // unrecognized construct is (not silently accepted).
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let fixture_path = tempdir.path().join("fixture.tr");
    std::fs::write(&fixture_path, r#"note { message: "hello" }"#).expect("failed to write fixture");

    let cli_output = Command::new(env!("CARGO_BIN_EXE_trust"))
        .arg(&fixture_path)
        .current_dir(tempdir.path())
        .output()
        .expect("failed to spawn the trust CLI");

    assert!(!cli_output.status.success(), "expected the CLI to reject an unrecognized 'note' construct");
}
