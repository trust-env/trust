use std::path::{Path, PathBuf};

use trust::{ai::AiConfig, generator::CodeGen, loader::load_project};

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    path
}

fn generate(entry: &Path, out: &Path) -> Result<(), String> {
    let asts = load_project(entry, &[])?;
    CodeGen::new(asts).run(out.to_path_buf(), false, &AiConfig::new_for_testing("http://127.0.0.1:1"))
}

#[test]
fn a_project_spans_several_files_with_file_relative_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let entry = write(
        tmp.path(),
        "api/main.tr",
        r#"include "./modules/tasks/tasks.tr"
include "./adapters/api.tr"
app Main: run { Api }
"#,
    );
    write(
        tmp.path(),
        "api/modules/tasks/tasks.tr",
        r#"include "../common.tr"
export type Task { id: Int, title: String, domain: Domain }
type Done { ok: Bool }
infra TaskInfra { storage { psql { entity: Task } } }
use_case CreateTaskUseCase { call: TaskInfra.save_one }
use_case PingUseCase {
    input: Done
    output: Done
    code {
        file: "./ping.rs"
        include: "../../shared/util.rs"
        deps { "chrono-tz": "0.10" }
    }
}
"#,
    );
    write(tmp.path(), "api/modules/common.tr", "export enum Domain { pro perso }\n");
    write(tmp.path(), "api/modules/tasks/ping.rs", "use common::Storage;\nOk(Done { ok: twice(1) == 2 })\n");
    write(tmp.path(), "api/shared/util.rs", "use common::UseCase;\npub fn twice(x: i64) -> i64 { x * 2 }\n");
    write(
        tmp.path(),
        "api/adapters/api.tr",
        r#"adapter Api { http { on: "0.0.0.0:8080" routes {
    POST "/tasks" { use_case: CreateTaskUseCase }
    POST "/ping" { use_case: PingUseCase }
} } }
"#,
    );

    let out = tmp.path().join("out");
    generate(&entry, &out).unwrap();

    let ping = std::fs::read_to_string(out.join("main/src/applications/ping_use_case.rs")).unwrap();
    assert!(ping.contains("Ok(Done { ok: twice(1) == 2 })"), "{ping}");
    // The prelude is a glob: the body's own `use common::Storage` is fine.
    assert!(ping.contains("use crate::*;") && !ping.contains("use common::{Storage"), "{ping}");
    assert!(out.join("main/src/shared/util.rs").is_file());
    assert!(std::fs::read_to_string(out.join("main/src/trust_prelude.rs")).unwrap().contains("pub use common::{Storage"));
    let cargo = std::fs::read_to_string(out.join("main/Cargo.toml")).unwrap();
    assert!(cargo.contains("chrono-tz = \"0.10\""), "{cargo}");
    // POST bodies accept an empty body.
    let adapter = std::fs::read_to_string(out.join("main/src/adapters/http/api.rs")).unwrap();
    assert!(adapter.contains("pub(crate) struct JsonBody<T>(pub T);"), "{adapter}");
}

#[test]
fn imported_types_resolve_against_their_own_module() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "domain.tr", "export enum Domain { pro perso }\nexport type Task { id: Int, domain: Domain, steps: List<Step> }\ntype Step { label: String }\n");
    let entry = write(
        tmp.path(),
        "main.tr",
        r#"import { Task } from "./domain.tr"
infra TaskInfra { storage { psql { entity: Task } } }
use_case CreateTaskUseCase { call: TaskInfra.save_one }
adapter Api { http { on: "0.0.0.0:8080" routes { POST "/tasks" { use_case: CreateTaskUseCase } } } }
app Main: run { Api }
"#,
    );
    let out = tmp.path().join("out");
    generate(&entry, &out).unwrap();
    // The types Task mentions are generated with it.
    for file in ["task.rs", "domain.rs", "step.rs"] {
        assert!(out.join("main/src/domains").join(file).is_file(), "missing {file}");
    }
}

#[test]
fn errors_point_at_file_line_and_column() {
    let tmp = tempfile::tempdir().unwrap();
    let entry = write(tmp.path(), "main.tr", "include \"./bad.tr\"\n");
    write(tmp.path(), "bad.tr", "type A {\n    x: Int\n    y: #\n}\n");
    let err = load_project(&entry, &[]).unwrap_err();
    assert!(err.contains("bad.tr:3:8: Unexpected character"), "{err}");
    assert!(err.contains("3 |     y: #"), "{err}");
}

#[test]
fn duplicate_declarations_across_files_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let entry = write(tmp.path(), "main.tr", "include \"./a.tr\"\ninclude \"./b.tr\"\n");
    write(tmp.path(), "a.tr", "type T { x: Int }\n");
    write(tmp.path(), "b.tr", "\ntype T { y: Int }\n");
    let err = load_project(&entry, &[]).unwrap_err();
    assert!(err.contains("type 'T' is declared twice") && err.contains("b.tr:2"), "{err}");
}
