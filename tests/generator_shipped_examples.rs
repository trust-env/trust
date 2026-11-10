#[path = "support/mod.rs"]
mod support;

/// Every `.tr` file shipped in `examples/` and `scenarios/` must generate on
/// a fresh clone: no import of a module that only exists on someone's
/// machine (the git-ignored `mod/` folder holds Rust checkouts, never `.tr`
/// modules). `ai {}` examples are skipped — they need a live provider.
#[test]
fn shipped_tr_files_generate_from_a_fresh_clone() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(root.join("examples"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "tr"))
        .collect();
    for scenario in std::fs::read_dir(root.join("scenarios")).unwrap() {
        let app = scenario.unwrap().path().join("app.tr");
        if app.exists() {
            files.push(app);
        }
    }
    files.sort();

    let mut checked = 0;
    for file in &files {
        let src = std::fs::read_to_string(file).unwrap();
        assert!(!src.contains("./mod/"), "{} imports from the git-ignored mod/ folder", file.display());
        if src.contains("ai {") || src.contains("ai{") {
            continue;
        }
        if let Err(error) = support::try_generate(&src) {
            panic!("{} failed to generate: {}", file.display(), error);
        }
        checked += 1;
    }
    assert!(checked >= 8, "expected the shipped examples to be found, checked {checked}");
}
