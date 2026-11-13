#[path = "support/mod.rs"]
mod support;

/// Generates `examples/<name>`, builds it for real, and fails on any rustc
/// warning in a use case file: the Rust trust writes for scripts is never
/// left for the user to fix, nor to silence.
fn assert_example_builds_cleanly(name: &str) {
    let src = std::fs::read_to_string(format!("{}/examples/{}", env!("CARGO_MANIFEST_DIR"), name))
        .unwrap();
    let project = support::generate(&src);
    support::patch_local_dependencies(&project);
    // A fresh project path: the crate is compiled, so its warnings show.
    let output = support::cargo_build(&project);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{} failed to build:\n{}",
        name,
        stderr
    );
    let warnings: Vec<&str> = stderr
        .split("\n\n")
        .filter(|block| block.starts_with("warning") && block.contains("src/applications/"))
        .collect();
    assert!(
        warnings.is_empty(),
        "{} has warnings in generated use cases:\n{}",
        name,
        warnings.join("\n\n")
    );
}

/// Most of the language: psql with owner scoping, a transaction, queries,
/// loops, lambdas, an http_client, JSON, a RawRequest webhook with HMAC.
#[test]
fn logic_example_compiles() {
    assert_example_builds_cleanly("logic_example.tr");
}

/// The app's time zone, `map` conversions, `journal("...")`, durations,
/// `String.take`, `??` between Options, `extern fn`, and running a
/// mirror's use case from a script.
#[test]
fn logic_journal_example_compiles() {
    assert_example_builds_cleanly("logic_journal_example.tr");
}
