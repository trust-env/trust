use crate::lexer::Lexer;
use crate::parser::{Ast, CodeSpec, Parser};

const PROJECT: &str = r#"
enum Status { todo done }
type Task { id: Int, user_id: Int, title: String, status: Status, points: Int, due: Option<DateTime>, tags: List<String> }
type Note { id: Int, text: String }
type Ref { id: Int }
type Out { count: Int, message: String }
type Text { text: String }
type Blob { key: String }
infra TaskInfra { storage { psql { entity: Task owner: user_id } } }
infra NoteInfra { storage { psql { entity: Note } } }
infra Api { http_client { base_url: "https://example.com" } }
infra Files { storage { s3 { bucket: "b" entity: Blob } } }
use_case CountUseCase { input: Ref output: Out logic { return Out { count: 1, message: "" } } }
"#;

/// The `code {}` a use case `input: Ref output: Out logic { <body> }`
/// becomes, or its error.
fn lower_with(header: &str, body: &str) -> Result<CodeSpec, String> {
    let src = format!(
        "{}\nuse_case SubjectUseCase {{ {} logic {{\n{}\n}} }}",
        PROJECT, header, body
    );
    let tokens = Lexer::new(&src).run()?;
    let asts = Parser::new(tokens).run()?;
    let asts = super::lower_all(asts)?;
    asts.into_iter()
        .find_map(|a| match a {
            Ast::UseCase(u) if u.name == "SubjectUseCase" => u.code,
            _ => None,
        })
        .ok_or_else(|| "no code".to_string())
}

fn lower(body: &str) -> Result<CodeSpec, String> {
    lower_with("input: Ref output: Out", body)
}

fn rust(body: &str) -> String {
    match lower(body) {
        Ok(code) => code.inline.unwrap(),
        Err(e) => panic!("{}", e),
    }
}

fn error(body: &str) -> String {
    match lower(body) {
        Ok(code) => panic!("expected an error, got:\n{}", code.inline.unwrap()),
        Err(e) => e,
    }
}

#[test]
fn psql_calls_await_propagate_and_are_scoped_to_their_owner() {
    let code = rust(
        "let t = TaskInfra.get(input.id)\nlet n = NoteInfra.find(input.id)\nreturn Out { count: t.points, message: t.title }",
    );
    assert!(code.contains("let t = self.task_infra.find_one_owned(input.id, \"user_id\", user).await?.ok_or(SubjectUseCaseError::NotFound)?;"), "{code}");
    assert!(
        code.contains("let n = self.note_infra.find_one(input.id).await?;"),
        "{code}"
    );
    assert!(
        code.contains("return Ok(Out { count: t.points, message: t.title.clone() });"),
        "{code}"
    );
}

#[test]
fn owner_none_reaches_every_row() {
    let code = lower_with(
        "input: Ref output: Out owner: none",
        "let t = TaskInfra.get(input.id)\nreturn Out { count: t.points, message: \"\" }",
    )
    .unwrap()
    .inline
    .unwrap();
    assert!(
        code.contains("self.task_infra.find_one(input.id)"),
        "{code}"
    );
}

#[test]
fn creates_and_updates_stamp_the_owner() {
    let code = rust(
        "let t = TaskInfra.create(Task { title: \"a\", status: Status.todo, points: 1, user_id: 0 })\nlet u = TaskInfra.update(Task { points: 2, ..t })\nreturn Out { count: u.points, message: \"\" }",
    );
    assert!(code.contains("let mut trust_row = Task { title: String::from(\"a\"), status: Status::Todo, points: 1i64, user_id: 0i64, ..Default::default() }; trust_row.user_id = user; self.task_infra.save_one(trust_row).await?"), "{code}");
    assert!(code.contains("update_one_owned(trust_row.id, \"user_id\", user, trust_row).await?.ok_or(SubjectUseCaseError::NotFound)?"), "{code}");
}

#[test]
fn queries_build_typed_conditions() {
    let code = rust(
        "let ts = TaskInfra.query(status != Status.done, points >= 3, title.contains(\"x\"), id in [1, 2], order_by_desc: points, limit: 10)\nreturn Out { count: ts.len(), message: \"\" }",
    );
    assert!(
        code.contains(r#"Query::new().eq("user_id", user).ne("status", Status::Done).gte("points", 3i64).contains("title", String::from("x")).is_in("id", vec![1i64, 2i64]).order_by_desc("points").limit(10i64)"#),
        "{code}"
    );
    assert!(
        error("let ts = TaskInfra.query(nope == 1)\nreturn Out { count: 0, message: \"\" }")
            .contains("'Task' has no field 'nope'")
    );
    assert!(
        error("let ts = TaskInfra.query(points == \"x\")\nreturn Out { count: 0, message: \"\" }")
            .contains("expected Int, found String")
    );
}

#[test]
fn errors_are_collected_from_fail() {
    let code = lower("if input.id == 0 { fail Invalid(\"zero: {input.id}\") }\nif input.id < 0 { fail NotFound }\nfail Conflict").unwrap();
    assert_eq!(code.errors, vec!["Invalid", "Conflict"]);
    let body = code.inline.unwrap();
    assert!(
        body.contains("return Err(SubjectUseCaseError::Invalid(format!(\"zero: {}\", input.id)));"),
        "{body}"
    );
    assert!(
        body.contains("return Err(SubjectUseCaseError::Conflict(String::from(\"Conflict\")));"),
        "{body}"
    );
}

#[test]
fn coalesce_unwraps_or_fails() {
    let code = rust(
        "let n = NoteInfra.find(input.id) ?? fail NotFound\nreturn Out { count: n.id, message: n.text }",
    );
    assert!(code.contains("let n = match self.note_infra.find_one(input.id).await? { Some(trust_value) => trust_value, None => return Err(SubjectUseCaseError::NotFound) };"), "{code}");
}

#[test]
fn lists_lambdas_and_loops() {
    let code = rust(
        "let ts = TaskInfra.all()\nvar total = 0\nfor t in ts { total = total + t.points }\nlet big = ts.filter(t => t.points > 2).map(t => t.title)\nreturn Out { count: total, message: big.join(\", \") }",
    );
    assert!(code.contains("for t in ts.iter() {"), "{code}");
    assert!(code.contains("total = total + t.points;"), "{code}");
    assert!(
        code.contains(
            "ts.iter().filter(|t| { let t = *t; t.points > 2i64 }).cloned().collect::<Vec<_>>()"
        ),
        "{code}"
    );
}

#[test]
fn http_responses_decode_into_what_the_context_wants() {
    let code = rust(
        "let r: Out = Api.get(\"/x\")\nlet j = Api.post(\"/y\", { id: input.id })\nreturn Out { count: j.count.int() ?? 0, message: r.message }",
    );
    assert!(
        code.contains("self.api.get_json::<Out>(&String::from(\"/x\")).await?"),
        "{code}"
    );
    assert!(
        code.contains("self.api.post_json::<_, serde_json::Value>("),
        "{code}"
    );
    assert!(code.contains("j[\"count\"].as_i64()"), "{code}");
}

#[test]
fn reached_infras_use_cases_and_crates_are_reported() {
    let code = lower("let o = CountUseCase.run(Ref { id: 1 })\nlet h = hmac_sha256(\"k\", \"m\")\nreturn Out { count: o.count, message: h }").unwrap();
    assert_eq!(code.use_cases, vec!["CountUseCase"]);
    assert!(code.deps.iter().any(|(n, _)| n == "hmac"));
    assert!(
        code.inline
            .unwrap()
            .contains("self.count_use_case.execute(user, Ref { id: 1i64 }).await?")
    );
}

#[test]
fn type_errors_point_at_the_script() {
    let e = error("let x = 1\nreturn Out { count: \"no\", message: \"\" }");
    // Without the loader, positions are relative to the block (the loader
    // places them in the `.tr` file: see tests/generator_logic.rs).
    assert!(
        e.contains("<logic>:3:21: UseCase 'SubjectUseCase' logic: expected Int, found String"),
        "{e}"
    );
    assert!(
        e.contains("return Out { count: \"no\", message: \"\" }"),
        "{e}"
    );
}

#[test]
fn every_path_must_return_or_fail() {
    assert!(
        error("if input.id > 0 { return Out { count: 1, message: \"\" } }")
            .contains("can reach its end without 'return' or 'fail'")
    );
    assert!(error("fail NotFound\nlet x = 1").contains("unreachable"));
    // Both branches returning is enough.
    rust("if input.id > 0 { return Out { count: 1, message: \"\" } } else { fail NotFound }");
}

#[test]
fn values_are_immutable_unless_var() {
    assert!(
        error("let x = 1\nx = 2\nfail NotFound")
            .contains("'x' can't change: declare it with 'var'")
    );
    assert!(
        error("var xs = [1]\nfor x in xs { xs.push(x) }\nfail NotFound")
            .contains("can't change while a 'for' loop walks it")
    );
    rust("var xs: List<Int> = []\nxs.push(1)\nreturn Out { count: xs.len(), message: \"\" }");
}

#[test]
fn fallible_calls_are_refused_inside_lambdas() {
    let e = error("let xs = [1, 2].map(x => NoteInfra.get(x).id)\nfail NotFound");
    assert!(
        e.contains(
            "calling NoteInfra can fail, so it can't run inside 'x => ...': use a 'for' loop"
        ),
        "{e}"
    );
    assert!(error("let xs = [1, 2].map(x => x / 2)\nfail NotFound").contains("can't run inside"));
}

#[test]
fn integer_division_is_checked() {
    let code = rust("return Out { count: input.id / 2, message: \"\" }");
    assert!(code.contains("input.id.checked_div(2i64).ok_or_else(|| SubjectUseCaseError::StorageError(String::from(\"division by zero\")))?"), "{code}");
}

#[test]
fn options_must_be_unwrapped() {
    assert!(
        error("let t = TaskInfra.get(1)\nif t.due < now() { fail NotFound }\nfail NotFound")
            .contains("on an Option: unwrap it first")
    );
    assert!(
        error("let n = NoteInfra.find(1)\nreturn Out { count: n.id, message: \"\" }")
            .contains("may be none")
    );
    let code = rust(
        "let t = TaskInfra.get(1)\nif let due = t.due { if due < now() { fail NotFound } }\nfail NotFound",
    );
    assert!(code.contains("if let Some(due) = t.due {"), "{code}");
}

#[test]
fn struct_literals_need_every_required_field() {
    let e = error("let t = Task { title: \"a\" }\nfail NotFound");
    assert!(e.contains("Task is missing status, points (only Option, List, id and owner fields may be left out)"), "{e}");
    assert!(
        error("let t = Note { text: \"a\", nope: 1 }\nfail NotFound")
            .contains("'Note' has no field 'nope'")
    );
}

#[test]
fn capabilities_are_limited_to_what_the_project_declares() {
    assert!(
        error("let x = env(input.id.to_string())\nfail NotFound")
            .contains("env() takes a literal UPPER_CASE name")
    );
    assert!(error("let f = Files.get(\"k\")\nfail NotFound").contains("logic can't use s3 infras"));
    assert!(error("let x = std(1)\nfail NotFound").contains("unknown function 'std'"));
    assert!(error("while true { }").contains("there is no 'while'"));
    assert!(error("if a && b { }").contains("unexpected '&&': write 'and'"));
}

#[test]
fn rust_keywords_are_valid_variable_names() {
    let code = rust("let type = 1\nreturn Out { count: type, message: \"\" }");
    assert!(code.contains("let r#type = 1i64;"), "{code}");
    assert!(error("let user = 1\nfail NotFound").contains("'user' is reserved"));
}

#[test]
fn strings_interpolate_and_escape() {
    let code = rust("return Out { count: 0, message: \"{input.id} {{x}} \\\"q\\\"\" }");
    assert!(
        code.contains(r#"format!("{} {{x}} \"q\"", input.id)"#),
        "{code}"
    );
    assert!(
        error("let t = TaskInfra.get(1)\nreturn Out { count: 0, message: \"{t.due}\" }")
            .contains("can't be written in a string: give it a default with '??'")
    );
}

#[test]
fn logic_is_exclusive_with_other_bodies() {
    let e = lower_with(
        "input: Ref output: Out call: NoteInfra.find_one",
        "fail NotFound",
    )
    .unwrap_err();
    assert!(e.contains("mutually exclusive"), "{e}");
}

#[test]
fn bare_strips_only_one_wrapping_pair() {
    assert_eq!(super::check::bare_for_tests("(a + b)"), "a + b");
    assert_eq!(super::check::bare_for_tests("(a) + (b)"), "(a) + (b)");
    assert_eq!(super::check::bare_for_tests("(\")\" == x)"), "\")\" == x");
}

const LOCAL_PROJECT: &str = r#"
config { tz: "Europe/Paris" }
type Item { id: Int, user_id: Int, text: String }
type Task { id: Int, user_id: Int, title: String, start_at: Option<DateTime> }
type Action { id: Int, user_id: Int, kind: String, text: String, changes: Json, undone: Bool, at: DateTime, requires_ack: Bool }
type Ref { id: Int }
type Out { n: Int, text: String }
infra ItemInfra { storage { psql { entity: Item owner: user_id } } }
infra TaskInfra { storage { psql { entity: Task owner: user_id } } }
infra ActionInfra { storage { psql { entity: Action owner: user_id } } }
journal Actions { infra: ActionInfra owner: user_id undo: "/actions/:id/toggle" conflict: refuse }
map Item -> Task { title: text }
"#;

/// Lowered through the whole desugar pass (journals, mirrors, ...).
fn local(header: &str, body: &str) -> Result<String, String> {
    let src = format!(
        "{}\nuse_case SubjectUseCase {{ input: Ref output: Out {} logic {{\n{}\n}} }}",
        LOCAL_PROJECT, header, body
    );
    let tokens = Lexer::new(&src).run()?;
    let asts = Parser::new(tokens).run()?;
    let asts = crate::generator::desugar::desugar(asts)?;
    asts.into_iter()
        .find_map(|a| match a {
            Ast::UseCase(u) if u.name == "SubjectUseCase" => u.code.and_then(|c| c.inline),
            _ => None,
        })
        .ok_or_else(|| "no code".to_string())
}

#[test]
fn dates_use_the_app_time_zone() {
    let code = local(
        "",
        "let d = today()\nlet t = now()\nlet ok = d.add_days(-1) < d and t.date() == d\nreturn Out { n: d.weekday() + t.hour() + t.minute(), text: \"{ok}\" }",
    )
    .unwrap();
    assert!(code.contains("let d = crate::tz::today();"), "{code}");
    assert!(
        code.contains("(d + chrono::Duration::days(-1i64)) < d"),
        "{code}"
    );
    assert!(code.contains("crate::tz::local(t).date_naive()"), "{code}");
    assert!(
        code.contains("chrono::Datelike::weekday(&d).number_from_monday() as i64"),
        "{code}"
    );
    assert!(
        code.contains("chrono::Timelike::hour(&crate::tz::local(t)) as i64"),
        "{code}"
    );
}

#[test]
fn durations_between_instants_and_dates() {
    let code = local(
        "",
        "let t = now()\nlet later = t.add_seconds(90)\nreturn Out { n: later.minutes_since(t) + later.seconds_since(t) + today().days_since(today()), text: \"\" }",
    )
    .unwrap();
    assert!(code.contains("(later - t).num_minutes()"), "{code}");
    assert!(code.contains("(later - t).num_seconds()"), "{code}");
    assert!(
        code.contains("((crate::tz::today()) - crate::tz::today()).num_days()"),
        "{code}"
    );
    let e = local(
        "",
        "return Out { n: today().minutes_since(today()), text: \"\" }",
    )
    .unwrap_err();
    assert!(e.contains("a Date has no method 'minutes_since'"), "{e}");
}

#[test]
fn strings_take_and_skip_characters() {
    let code = local(
        "",
        "return Out { n: 0, text: \"héllo\".take(2) + \"x\".skip(1) }",
    )
    .unwrap();
    assert!(
        code.contains(".chars().take(usize::try_from(2i64).unwrap_or(0)).collect::<String>()"),
        "{code}"
    );
}

#[test]
fn map_conversions_are_methods() {
    let code = local(
        "",
        "let item = ItemInfra.get(input.id)\nlet t = TaskInfra.create(item.to_task())\nreturn Out { n: t.id, text: t.title }",
    )
    .unwrap();
    assert!(code.contains("trust_row = item.to_task();"), "{code}");
    let e = local(
        "",
        "let item = ItemInfra.get(input.id)\nlet t = item.to_note()\nfail NotFound",
    )
    .unwrap_err();
    assert!(e.contains("a Item has no method 'to_note'"), "{e}");
}

#[test]
fn journal_names_the_entry_of_journaled_use_cases() {
    let code = local(
        "journal: Actions",
        "journal(\"done {input.id}\")\nreturn Out { n: 0, text: \"\" }",
    )
    .unwrap();
    assert!(
        code.contains("self.trust_journal.text(format!(\"done {}\", input.id));"),
        "{code}"
    );
    let e = local("", "journal(\"x\")\nfail NotFound").unwrap_err();
    assert!(
        e.contains("journal() names the entry of a use case with 'journal: <Journal>'"),
        "{e}"
    );
}

#[test]
fn use_cases_added_by_desugaring_can_be_run() {
    // The journal's undo use case only exists after desugaring.
    let code = local(
        "",
        "let a = ToggleActionsUseCase.run(ActionsToggleRequest { id: input.id })\nreturn Out { n: a.id, text: a.text }",
    )
    .unwrap();
    assert!(code.contains("self.toggle_actions_use_case.execute(user, ActionsToggleRequest { id: input.id }).await?"), "{code}");
}

#[test]
fn coalesce_fallbacks_have_no_extra_parentheses() {
    let code = rust(
        "let n = NoteInfra.find(input.id)\nlet m = n ?? Note { text: \"\" }\nreturn Out { count: m.id + (input.id.to_string().to_int() ?? input.id + 1), message: \"\" }",
    );
    assert!(!code.contains("None => ("), "{code}");
    let code = rust("let t = TaskInfra.get(1)\nlet d = t.due ?? now().add_days(1)\nfail NotFound");
    assert!(
        code.contains("None => (chrono::Utc::now()) + chrono::Duration::days(1i64) }"),
        "{code}"
    );
}

#[test]
fn coalescing_two_options_is_an_option() {
    let code = rust(
        "let ts = TaskInfra.all()\nlet t = ts.find(t => t.points > 3) ?? ts.find(t => t.points > 1)\nlet u = ts.first() ?? ts.last() ?? fail NotFound\nreturn Out { count: (t ?? u).points, message: \"\" }",
    );
    assert!(
        code.contains("Some(trust_value) => Some(trust_value), None => ts.iter().find("),
        "{code}"
    );
    // Right-associative: `a ?? b ?? fail X` is `a ?? (b ?? fail X)`.
    assert!(code.contains("None => match ts.last().cloned() { Some(trust_value) => trust_value, None => return Err("), "{code}");
}

#[test]
fn owner_fields_may_be_left_out_of_literals() {
    let code = rust(
        "let t = TaskInfra.create(Task { title: \"a\", status: Status.todo, points: 1 })\nreturn Out { count: t.id, message: \"\" }",
    );
    assert!(
        code.contains("..Default::default() }; trust_row.user_id = user;"),
        "{code}"
    );
    // Note has no owner: nothing to fill for it.
    assert!(error("let n = Note { }\nfail NotFound").contains("Note is missing text"));
}

/// A project with a top-level `logic { extern fn ... }` over `rules.rs`.
fn with_library(library: &str, body: &str) -> Result<CodeSpec, String> {
    let dir = tempfile::tempdir().unwrap();
    let rules = dir.path().join("rules.rs");
    std::fs::write(
        &rules,
        "pub fn score(title: &str, points: i64) -> i64 { title.len() as i64 + points }\n",
    )
    .unwrap();
    let library = library.replace("@RULES@", &rules.display().to_string());
    let src = format!(
        "{}\nlogic {{\n{}\n}}\nuse_case SubjectUseCase {{ input: Ref output: Out logic {{\n{}\n}} }}",
        PROJECT, library, body
    );
    let tokens = Lexer::new(&src).run()?;
    let asts = Parser::new(tokens).run()?;
    let asts = super::lower_all(asts)?;
    assert!(
        !asts.iter().any(|a| matches!(a, Ast::LogicLibrary(_))),
        "libraries are consumed by lowering"
    );
    asts.into_iter()
        .find_map(|a| match a {
            Ast::UseCase(u) if u.name == "SubjectUseCase" => u.code,
            _ => None,
        })
        .ok_or_else(|| "no code".to_string())
}

#[test]
fn extern_fns_call_declared_rust_and_include_its_file() {
    let code = with_library(
        "extern fn score(title: &String, points: Int) -> Int from \"@RULES@\"",
        "let t = TaskInfra.get(input.id)\nlet all = TaskInfra.all().map(x => score(x.title, x.points))\nreturn Out { count: score(t.title, t.points) + score(\"x\" + \"y\", 1) + all.sum(), message: \"\" }",
    )
    .unwrap();
    let body = code.inline.unwrap();
    assert!(body.contains("crate::score(&t.title, t.points)"), "{body}");
    // A lambda parameter is already a reference.
    assert!(body.contains("crate::score(&x.title, x.points)"), "{body}");
    assert!(
        body.contains(
            "crate::score(&format!(\"{}{}\", String::from(\"x\"), String::from(\"y\")), 1i64)"
        ),
        "{body}"
    );
    assert_eq!(code.includes.len(), 1);
    assert!(code.includes[0].ends_with("rules.rs"));
}

#[test]
fn extern_fns_are_type_checked() {
    let decl = "extern fn score(title: &String, points: Int) -> Int from \"@RULES@\"";
    let e = with_library(decl, "return Out { count: score(1, 2), message: \"\" }").unwrap_err();
    assert!(e.contains("expected String, found Int"), "{e}");
    let e = with_library(decl, "return Out { count: score(\"a\"), message: \"\" }").unwrap_err();
    assert!(e.contains("expected score(&String, Int)"), "{e}");
    let e = with_library("extern fn now() -> Int from \"@RULES@\"", "fail NotFound").unwrap_err();
    assert!(e.contains("'now' is a built-in function"), "{e}");
    let e = with_library(&format!("{}\n{}", decl, decl), "fail NotFound").unwrap_err();
    assert!(e.contains("extern fn 'score' is declared twice"), "{e}");
    let e = with_library(
        "extern fn f(a: Int) -> Int from \"./nope.rs\"",
        "fail NotFound",
    )
    .unwrap_err();
    assert!(e.contains("nope.rs' doesn't exist"), "{e}");
    let e = with_library(
        "extern fn f(a: Int) -> Nope from \"@RULES@\"",
        "fail NotFound",
    )
    .unwrap_err();
    assert!(e.contains("unknown type 'Nope'"), "{e}");
}

#[test]
fn try_turns_a_failure_into_a_value() {
    let code = rust(
        "let r = try CountUseCase.run(Ref { id: 1 })\nif let e = r.error { fail Broken(\"count: {e}\") }\nlet n = try Api.get(\"/n\") ?? to_json(0)\nreturn Out { count: (r ?? Out { count: 0, message: \"\" }).count, message: \"{r.is_ok()} {n}\" }",
    );
    assert!(code.starts_with("// Generated by trust"), "{code}");
    assert!(
        code.contains("let trust_error_text = |error: SubjectUseCaseError| -> String { match error { SubjectUseCaseError::NotFound => String::from(\"not found\"), SubjectUseCaseError::StorageError(message) => message, SubjectUseCaseError::Broken(message) => message } };"),
        "{code}"
    );
    assert!(
        code.contains("let r = async { Ok::<_, SubjectUseCaseError>(self.count_use_case.execute(user, Ref { id: 1i64 }).await?) }.await.map_err(trust_error_text);"),
        "{code}"
    );
    assert!(
        code.contains("if let Some(e) = r.clone().err() {"),
        "{code}"
    );
    assert!(
        code.contains(
            "match async { Ok::<_, SubjectUseCaseError>(self.api.get_json::<serde_json::Value>("
        ),
        "{code}"
    );
    assert!(
        error("let xs = [1].map(x => try CountUseCase.run(Ref { id: x }))\nfail NotFound")
            .contains("can't run inside")
    );
}

#[test]
fn raw_http_calls_never_fail_on_the_status() {
    let code = rust(
        "let r = Api.send(\"PATCH\", \"/x\", { a: 1 })\nlet d = Api.send(\"DELETE\", \"/y\")\nreturn Out { count: r.status + d.status, message: r.text + r.json().a.to_string() }",
    );
    assert!(code.contains("self.api.send_raw(self.api.request(reqwest::Method::PATCH, &String::from(\"/x\")).json(&{"), "{code}");
    assert!(
        code.contains("(trust_status.as_u16() as i64, trust_text)"),
        "{code}"
    );
    assert!(
        error("let r = Api.send(\"FETCH\", \"/x\")\nfail NotFound")
            .contains("send() takes a literal \"GET\"")
    );
    let code = rust(
        "let items: List<Note> = Api.get_paged(\"/n\", \"items\", \"next\")\nreturn Out { count: items.len(), message: \"{Api.is_configured()}\" }",
    );
    assert!(code.contains("self.api.get_json_paged::<Note>(&String::from(\"/n\"), &String::from(\"items\"), &String::from(\"next\")).await?"), "{code}");
    assert!(code.contains("self.api.is_configured()"), "{code}");
    assert!(
        error("let o = Api.occurrences(now(), now())\nfail NotFound")
            .contains("occurrences() reads an ics_feed infra")
    );
}

#[test]
fn environment_urls_dates_and_logs() {
    let code = rust(
        "let id = env_opt(\"CALENDAR_ID\") ?? \"primary\"\nlet t = parse_datetime(\"2026-01-02\") ?? week_start()\nlet d = parse_date(\"2026-01-02T10:00:00Z\")\nlog_warn(\"id {url_encode(id)}\")\nreturn Out { count: 0, message: \"{t} {d ?? today()}\" }",
    );
    assert!(code.contains("std::env::var(\"CALENDAR_ID\").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())"), "{code}");
    assert!(code.contains("chrono::DateTime::parse_from_rfc3339(trust_text).ok().map(|d| d.with_timezone(&chrono::Utc)).or_else(|| chrono::NaiveDate::parse_from_str(trust_text, \"%Y-%m-%d\").ok().map(|d| d.and_time(chrono::NaiveTime::MIN).and_utc()))"), "{code}");
    assert!(code.contains("tracing::warn!(use_case = \"SubjectUseCase\", \"{}\", format!(\"id {}\", urlencoding::encode(&id).into_owned()));"), "{code}");
    assert!(error("let x = env_opt(\"lower\")\nfail NotFound").contains("literal UPPER_CASE name"));
    // In the app's time zone when there is one.
    let code = local(
        "",
        "let t = week_start()\nlet p = parse_datetime(\"2026-01-02\")\nfail NotFound",
    )
    .unwrap();
    assert!(
        code.contains("let t = crate::tz::week_bounds(chrono::Utc::now()).0;"),
        "{code}"
    );
    assert!(code.contains(".map(|d| crate::tz::midnight(d))"), "{code}");
}

#[test]
fn lists_take_one_item_per_line_without_commas() {
    let code = rust("let xs = [\n  1\n  2,\n  3\n]\nreturn Out { count: xs.len(), message: \"\" }");
    assert!(code.contains("let xs = vec![1i64, 2i64, 3i64];"), "{code}");
    assert!(
        error("let xs = [1 2]\nfail NotFound")
            .contains("expected ',' or a new line between list items")
    );
}

const PLANNER: &str = r#"
config { tz: "Europe/Paris" locale: "fr" }
enum Kind { brief conflict }
enum Status { sent failed }
enum Change { move { id: Int, start: DateTime, note: Option<String> } reset }
type Event { id: Int, user_id: Int, title: String, project: Int, hours: Float, start_at: DateTime }
type Action { id: Int, user_id: Int, kind: String, text: String, changes: Json, undone: Bool, at: DateTime, requires_ack: Bool }
type Notification { id: Int, user_id: Int, kind: Kind, at: DateTime, channel: String, text: String, status: Status, error: Option<String>, external_id: Option<String> }
type Ref { id: Int }
type Out { n: Int, text: String }
infra EventInfra { storage { psql { entity: Event owner: user_id } } }
infra ActionInfra { storage { psql { entity: Action owner: user_id } } }
infra NotificationInfra { storage { psql { entity: Notification owner: user_id } } }
journal Actions { infra: ActionInfra owner: user_id undo: "/actions/:id/toggle" conflict: refuse }
channel Telegram { telegram { token: "${T}" chat: "${C}" } }
notifications Notify { infra: NotificationInfra default: Telegram fallback: log }
"#;

/// Lowered through desugaring, in the planner project.
fn planner(header: &str, body: &str) -> Result<String, String> {
    let src = format!(
        "{}\nuse_case SubjectUseCase {{ input: Ref output: Out {} logic {{\n{}\n}} }}",
        PLANNER, header, body
    );
    let asts = Parser::new(Lexer::new(&src).run()?).run()?;
    crate::generator::desugar::desugar(asts)?
        .into_iter()
        .find_map(|a| match a {
            Ast::UseCase(u) if u.name == "SubjectUseCase" => u.code.and_then(|c| c.inline),
            _ => None,
        })
        .ok_or_else(|| "no code".to_string())
}

#[test]
fn maps_group_and_count() {
    let code = planner(
        "",
        "let events = EventInfra.all()\nvar hours: Map<Int, Float> = {}\nfor e in events { hours.set(e.project, (hours.get(e.project) ?? 0.0) + e.hours) }\nlet groups = events.group_by(e => e.title)\nlet by_id = events.to_map(e => e.id)\nreturn Out { n: hours.len() + groups.keys().len() + by_id.values().len() + [1, 1, 2].distinct().len(), text: \"{hours.contains(1)}\" }",
    )
    .unwrap();
    assert!(
        code.contains("let mut hours: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::<i64, f64>::new();"),
        "{code}"
    );
    assert!(code.contains("hours.insert(e.project, "), "{code}");
    assert!(
        code.contains(
            "trust_map.entry(trust_key(trust_item)).or_default().push(trust_item.clone());"
        ),
        "{code}"
    );
    assert!(
        code.contains("trust_map.insert(trust_key(trust_item), trust_item.clone());"),
        "{code}"
    );
    let e = planner("", "let m: Map<Float, Int> = {}\nfail NotFound").unwrap_err();
    assert!(
        e.contains("a Map key is an Int, String, Bool, Date, DateTime or Uuid, not Float"),
        "{e}"
    );
    let e = planner("", "let m: Map<Int, Int> = {}\nm.set(1, 2)\nfail NotFound").unwrap_err();
    assert!(e.contains("'m' can't change: declare it with 'var'"), "{e}");
}

#[test]
fn enums_with_fields_are_built_tested_and_destructured() {
    let code = planner(
        "",
        "let c = Change.move { id: 1, start: now() }\nlet r = Change.reset\nif let Change.move { id, start: at } = c {\n  return Out { n: id, text: \"{at} {c.is(Change.move)} {r.is(Change.reset)}\" }\n}\nfail NotFound",
    )
    .unwrap();
    assert!(
        code.contains("let c = Change::Move { id: 1i64, start: chrono::Utc::now(), note: None };"),
        "{code}"
    );
    assert!(code.contains("let r = Change::Reset;"), "{code}");
    assert!(
        code.contains("if let Change::Move { id, start: at, .. } = c.clone() {"),
        "{code}"
    );
    assert!(code.contains("matches!(c, Change::Move { .. })"), "{code}");
    let e = planner("", "let c = Change.move { id: 1 }\nfail NotFound").unwrap_err();
    assert!(e.contains("Change.move is missing start"), "{e}");
    let e = planner("", "let c = Change.jump\nfail NotFound").unwrap_err();
    assert!(e.contains("'Change' has no value 'jump'"), "{e}");
}

#[test]
fn journal_entries_dates_and_notifications() {
    let code = planner(
        "journal: Actions",
        "journal_entry(\"moved {now().format(\"EEE dd/MM\")}\", requires_ack: true)\nlet sent = Notify.send_recorded(Kind.brief, \"hello\")\nNotify.send(\"conflict\", \"x\")\nreturn Out { n: sent.id, text: \"{sent.status == Status.sent}\" }",
    )
    .unwrap();
    assert!(code.contains("self.trust_journal.entry(format!(\"moved {}\", crate::fmt::fmt_date((chrono::Utc::now()), &String::from(\"EEE dd/MM\")))).requires_ack(true);"), "{code}");
    assert!(code.contains("self.notify.send_recorded(&self.notification_infra, user, Kind::Brief.as_str(), &String::from(\"hello\")).await?"), "{code}");
    assert!(
        code.contains(
            "let _ = self.notify.send(&String::from(\"conflict\"), &String::from(\"x\")).await;"
        ),
        "{code}"
    );
    let e = planner("", "journal_entry(\"x\")\nfail NotFound").unwrap_err();
    assert!(
        e.contains("journal_entry() starts an entry of a use case with 'journal: <Journal>'"),
        "{e}"
    );
    // `format` needs the app's language.
    let e = local("", "let s = now().format(\"dd\")\nfail NotFound").unwrap_err();
    assert!(e.contains("declare config { locale"), "{e}");
}

#[test]
fn filter_map_keeps_set_values() {
    let code = rust(
        "let ts = TaskInfra.all()\nlet dues = ts.filter_map(t => t.due)\nlet n = [\"1\", \"x\"].filter_map(s => s.to_int())\nreturn Out { count: dues.len() + n.sum(), message: \"\" }",
    );
    assert!(
        code.contains("let dues = ts.iter().filter_map(|t| t.due).collect::<Vec<_>>();"),
        "{code}"
    );
    assert!(
        error("let xs = [1].filter_map(x => x)\nfail NotFound")
            .contains("filter_map() keeps the set values of an Option, found Int")
    );
}

#[test]
fn strings_nest_inside_interpolations() {
    let code = planner(
        "",
        "return Out { n: 0, text: \"at {now().format(\"HH:mm {{x}}\")} ok\" }",
    )
    .unwrap();
    assert!(
        code.contains("crate::fmt::fmt_date((chrono::Utc::now()), &String::from(\"HH:mm {x}\"))"),
        "{code}"
    );
}
