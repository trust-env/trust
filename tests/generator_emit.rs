#[path = "support/mod.rs"]
mod support;

use support::parse;
use trust::generator::CodeGen;

const TR: &str = r#"config { json_case: "camel" }
enum TaskStatus { todo done }
type Task { id: Int, user_id: Int, project_id: Option<Int>, title: String, status: TaskStatus }
infra TaskInfra { storage { psql { entity: Task } } }
use_case CreateTaskUseCase { call: TaskInfra.save_one owner: user_id }
use_case ListTasksUseCase { call: TaskInfra.list owner: user_id filters { project_id: input.project_id } }
adapter Api {
    http {
        on: "0.0.0.0:8080"
        envelope: false
        routes {
            POST "/tasks" { use_case: CreateTaskUseCase auth: required }
            GET "/projects/:project_id/tasks" { use_case: ListTasksUseCase auth: required }
        }
    }
}
app Main: run { Api }
"#;

#[test]
fn typescript_client_mirrors_the_server() {
    let ts = CodeGen::new(Vec::new()).emit_typescript(&parse(TR)).unwrap();
    assert!(ts.contains("export type TaskStatus = \"todo\" | \"done\";"), "{ts}");
    assert!(ts.contains("  projectId?: number | null;"), "{ts}");
    assert!(ts.contains("export class ApiClient extends TrustClient {"), "{ts}");
    assert!(ts.contains("createTask(input: CreateTaskUseCaseInput): Promise<Task> {"), "{ts}");
    assert!(ts.contains("listTasks(input: ListTasksUseCaseInput): Promise<Task[]> {"), "{ts}");
    assert!(ts.contains("`/projects/${encodeURIComponent(String(input.projectId))}/tasks`"), "{ts}");
    assert!(ts.contains("omit(input, [\"projectId\"])"), "{ts}");
}

#[test]
fn openapi_describes_paths_parameters_and_security() {
    let doc: serde_json::Value =
        serde_json::from_str(&CodeGen::new(Vec::new()).emit_openapi(&parse(TR), "api").unwrap()).unwrap();
    assert_eq!(doc["openapi"], "3.1.0");
    let list = &doc["paths"]["/projects/{project_id}/tasks"]["get"];
    assert_eq!(list["operationId"], "ListTasksUseCase");
    assert_eq!(list["parameters"][0]["in"], "path");
    assert_eq!(list["security"][0]["bearer"], serde_json::json!([]));
    assert_eq!(list["responses"]["200"]["content"]["application/json"]["schema"]["type"], "array");
    let create = &doc["paths"]["/tasks"]["post"];
    assert_eq!(create["requestBody"]["content"]["application/json"]["schema"]["properties"]["status"]["enum"], serde_json::json!(["todo", "done"]));
}
