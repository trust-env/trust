//! Embeds every `templates/**/*.tera` file into the compiler binary: writes
//! `$OUT_DIR/templates.rs`, a `&[(name, content)]` list of `include_str!`s
//! that `src/generator/rust_code/render.rs` registers with Tera. Adding a
//! template is just adding the file.

use std::{env, fs, path::Path};

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", dir.display(), e))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "tera") {
            let name = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.push(name);
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    println!("cargo:rerun-if-changed=templates");
    let mut names = Vec::new();
    collect(&root, &root, &mut names);

    let mut code = String::from("pub const BUNDLED_TEMPLATES: &[(&str, &str)] = &[\n");
    for name in &names {
        code.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            name,
            root.join(name).to_string_lossy()
        ));
    }
    code.push_str("];\n");
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("templates.rs");
    fs::write(out, code).unwrap();
}
