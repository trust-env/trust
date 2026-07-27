use std::collections::HashMap;

/// Configuration for `ai {}` blocks, built from CLI flags (e.g.
/// `--ai-api-key=...`) rather than hardcoded — every credential/parameter
/// the feature needs is passed in that way (env vars are only a fallback
/// for the API key, so local runs don't have to put a secret on the command
/// line if they'd rather not).
pub struct AiConfig {
    provider: String,
    api_key: Option<String>,
    model: String,
    /// Overridable so tests can point this at a local mock server instead
    /// of the real Anthropic API. Defaults to the real API's base URL.
    base_url: String,
}

impl AiConfig {
    pub fn from_flags(flags: &HashMap<String, String>) -> Self {
        AiConfig {
            provider: flags
                .get("ai-provider")
                .cloned()
                .unwrap_or_else(|| "anthropic".to_string()),
            api_key: flags
                .get("ai-api-key")
                .cloned()
                .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
            model: flags
                .get("ai-model")
                .cloned()
                .unwrap_or_else(|| "claude-sonnet-4-5".to_string()),
            base_url: flags
                .get("ai-base-url")
                .cloned()
                .unwrap_or_else(|| "https://api.anthropic.com".to_string()),
        }
    }

    /// Builds a config pointing at `base_url` (e.g. a local mock server)
    /// instead of the real Anthropic API — not gated behind `#[cfg(test)]`
    /// so it's usable from external integration/e2e test crates too, which
    /// link against this crate as a normal (non-test) dependency.
    pub fn new_for_testing(base_url: impl Into<String>) -> Self {
        AiConfig {
            provider: "anthropic".to_string(),
            api_key: Some("test-key".to_string()),
            model: "test-model".to_string(),
            base_url: base_url.into(),
        }
    }

    /// Asks the configured provider to write a function body given
    /// `context` (a description of everything the function is allowed to
    /// use: the input/output shapes, the storage methods available on
    /// `self`, and its error type) and `prompt` (what the `.tr` file asked
    /// for). Returns the Rust statements to splice into the function body,
    /// with any ``` fences the model added stripped.
    pub fn generate_body(&self, context: &str, prompt: &str) -> Result<String, String> {
        let api_key = self.api_key.as_deref().ok_or_else(|| {
            "An 'ai {}' block needs an API key: pass --ai-api-key=<key> (or set ANTHROPIC_API_KEY)"
                .to_string()
        })?;

        match self.provider.as_str() {
            "anthropic" => Self::call_anthropic(api_key, &self.model, &self.base_url, context, prompt),
            other => Err(format!(
                "Unsupported --ai-provider '{}': only 'anthropic' is supported today",
                other
            )),
        }
    }

    fn call_anthropic(api_key: &str, model: &str, base_url: &str, context: &str, prompt: &str) -> Result<String, String> {
        let full_prompt = format!(
            "You are writing the body of a single Rust async function inside a generated web service.\n\
Only output the Rust statements that belong inside the function body (what would go between its\n\
braces) — no function signature, no markdown code fences, no explanation, just the raw code.\n\
The function must end in an expression or statement that produces its declared return type.\n\n\
Context (exactly what the function has available):\n{context}\n\nTask: {prompt}\n",
            context = context,
            prompt = prompt,
        );

        let response = ureq::post(format!("{}/v1/messages", base_url))
            .config()
            .http_status_as_error(false)
            .build()
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .send_json(serde_json::json!({
                "model": model,
                "max_tokens": 1536,
                "messages": [{ "role": "user", "content": full_prompt }],
            }))
            .map_err(|e| format!("Failed to reach Anthropic API: {}", e));

        let mut response = response?;
        let status = response.status();
        let body: serde_json::Value = response
            .body_mut()
            .read_json()
            .map_err(|e| format!("Failed to parse Anthropic API response: {}", e))?;

        if !status.is_success() {
            let message = body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(format!("Anthropic API error ({}): {}", status, message));
        }

        let text = body
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| "Anthropic API response did not contain any text content".to_string())?;

        Ok(Self::strip_code_fences(text))
    }

    fn strip_code_fences(text: &str) -> String {
        let trimmed = text.trim();
        let Some(rest) = trimmed.strip_prefix("```") else {
            return trimmed.to_string();
        };
        let rest = rest.strip_prefix("rust").unwrap_or(rest);
        let rest = rest.trim_start_matches('\n');

        match rest.rfind("```") {
            Some(end) => rest[..end].trim().to_string(),
            None => rest.trim().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_fence_with_rust_language_tag() {
        let text = "```rust\nOk(Response { value: 1 })\n```";
        assert_eq!(AiConfig::strip_code_fences(text), "Ok(Response { value: 1 })");
    }

    #[test]
    fn strips_fence_without_language_tag() {
        let text = "```\nOk(Response { value: 1 })\n```";
        assert_eq!(AiConfig::strip_code_fences(text), "Ok(Response { value: 1 })");
    }

    #[test]
    fn passes_through_unfenced_text() {
        let text = "Ok(Response { value: 1 })";
        assert_eq!(AiConfig::strip_code_fences(text), "Ok(Response { value: 1 })");
    }

    #[test]
    fn trims_surrounding_whitespace_on_unfenced_text() {
        let text = "  \n Ok(Response { value: 1 })  \n";
        assert_eq!(AiConfig::strip_code_fences(text), "Ok(Response { value: 1 })");
    }

    #[test]
    fn from_flags_defaults_to_real_anthropic_base_url() {
        let flags = HashMap::new();
        let config = AiConfig::from_flags(&flags);
        assert_eq!(config.base_url, "https://api.anthropic.com");
    }

    #[test]
    fn from_flags_honors_ai_base_url_override() {
        let mut flags = HashMap::new();
        flags.insert("ai-base-url".to_string(), "http://127.0.0.1:9999".to_string());
        let config = AiConfig::from_flags(&flags);
        assert_eq!(config.base_url, "http://127.0.0.1:9999");
    }

    #[test]
    fn missing_api_key_produces_actionable_error() {
        // Built directly (not via from_flags/env) so this doesn't depend on
        // whether ANTHROPIC_API_KEY happens to be set in the test process.
        let config = AiConfig {
            provider: "anthropic".to_string(),
            api_key: None,
            model: "test-model".to_string(),
            base_url: "http://127.0.0.1:9999".to_string(),
        };
        let err = config.generate_body("context", "prompt").unwrap_err();
        assert!(err.contains("--ai-api-key"));
        assert!(err.contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn unsupported_provider_produces_clear_error() {
        let config = AiConfig {
            provider: "openai".to_string(),
            api_key: Some("key".to_string()),
            model: "test-model".to_string(),
            base_url: "http://127.0.0.1:9999".to_string(),
        };
        let err = config.generate_body("context", "prompt").unwrap_err();
        assert!(err.contains("openai"));
        assert!(err.contains("anthropic"));
    }
}
