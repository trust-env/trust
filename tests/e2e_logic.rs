#[path = "support/mod.rs"]
mod support;

/// `examples/logic_example.tr` uses most of the `logic {}` language (psql
/// with owner scoping, a transaction, queries, loops, lambdas, an
/// http_client, JSON, a RawRequest webhook with HMAC): its generated crate
/// must build — the Rust trust writes for scripts is never left for the
/// user to fix.
#[test]
fn logic_example_compiles() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/logic_example.tr"
    ))
    .unwrap();
    let project = support::generate(&src);
    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}

/// `examples/logic_journal_example.tr`: the app's time zone, `map`
/// conversions, `journal("...")`, durations, `String.take`, and running a
/// mirror's use case from a script.
#[test]
fn logic_journal_example_compiles() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/logic_journal_example.tr"
    ))
    .unwrap();
    let project = support::generate(&src);
    support::patch_local_dependencies(&project);
    support::assert_build_succeeds(&project);
}
