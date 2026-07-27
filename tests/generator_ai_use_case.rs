#[path = "support/mod.rs"]
mod support;

use trust::ai::AiConfig;

#[test]
fn ai_use_case_generates_a_use_case_with_the_ai_authored_body_spliced_in() {
    let base_url = support::mock_anthropic_returning("Ok(GreetResponse { greeting: \"test\".to_string() })");
    let ai_config = AiConfig::new_for_testing(base_url);

    let project = support::generate_with_ai(
        r#"type GreetRequest {
            name: String
        }
        type GreetResponse {
            greeting: String
        }
        use_case GreetUseCase {
            input: GreetRequest
            output: GreetResponse
            ai {
                prompt: "return a friendly greeting for the input name"
            }
        }"#,
        &ai_config,
    );

    let use_case = project.read("src/applications/greet_use_case.rs");
    assert!(use_case.contains("Ok(GreetResponse { greeting: \"test\".to_string() })"));
    assert!(use_case.contains("pub struct GreetUseCase;"));
    assert!(use_case.contains("enum GreetUseCaseError"));
}

#[test]
fn ai_use_case_with_infra_gets_a_storage_backed_struct() {
    let base_url = support::mock_anthropic_returning("Ok(SummarizeResponse { summary: input.text })");
    let ai_config = AiConfig::new_for_testing(base_url);

    let project = support::generate_with_ai(
        r#"type Doc {
            id: Int,
            text: String
        }
        type SummarizeRequest {
            text: String
        }
        type SummarizeResponse {
            summary: String
        }
        infra DocInfra {
            storage {
                psql {
                    entity: Doc
                }
            }
        }
        use_case SummarizeUseCase {
            input: SummarizeRequest
            output: SummarizeResponse
            ai {
                infra: DocInfra
                prompt: "summarize the input text"
            }
        }"#,
        &ai_config,
    );

    let use_case = project.read("src/applications/summarize_use_case.rs");
    assert!(use_case.contains("doc_infra: S0"));
    assert!(use_case.contains("DocStorage"));
}
