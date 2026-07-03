use crate::config::Config;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EndpointKind {
    Responses,
    ChatCompletions,
}

pub(super) fn apply_provider_body_defaults(
    _cfg: &Config,
    endpoint_kind: EndpointKind,
    body: &mut Value,
) {
    if endpoint_kind == EndpointKind::Responses && body.get("reasoning").is_none() {
        body["reasoning"] = json!({ "effort": "low" });
    }
}

#[cfg(test)]
mod tests {
    use super::{EndpointKind, apply_provider_body_defaults};
    use crate::config::{Config, Provider, default_redaction_rules};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn adds_minimal_reasoning_for_all_responses_requests() {
        let cfg = sample_config("gpt-4.1-mini");
        let mut body = json!({
            "model": "gpt-4.1-mini",
            "input": "hello"
        });

        apply_provider_body_defaults(&cfg, EndpointKind::Responses, &mut body);

        assert_eq!(body["reasoning"], json!({ "effort": "low" }));
    }

    #[test]
    fn does_not_override_explicit_reasoning() {
        let cfg = sample_config("gpt-4.1-mini");
        let mut body = json!({
            "model": "gpt-4.1-mini",
            "reasoning": { "effort": "high" }
        });

        apply_provider_body_defaults(&cfg, EndpointKind::Responses, &mut body);

        assert_eq!(body["reasoning"], json!({ "effort": "high" }));
    }

    #[test]
    fn leaves_chat_requests_unchanged() {
        let cfg = sample_config("gpt-4.1-mini");
        let mut body = json!({
            "model": "gpt-4.1-mini",
            "messages": []
        });

        apply_provider_body_defaults(&cfg, EndpointKind::ChatCompletions, &mut body);

        assert!(body.get("reasoning").is_none());
    }

    fn sample_config(model: &str) -> Config {
        Config {
            provider: Provider::OpenAiCompatible,
            api_base: "https://ai.cloud1ful.com/v1".to_string(),
            api_key: "token".to_string(),
            model: model.to_string(),
            confirm_commit: true,
            open_editor: false,
            enable_fallback: false,
            redact_secrets: true,
            redaction_rules: default_redaction_rules(),
            show_timing: true,
            use_env_proxy: false,
            timeout: Duration::from_secs(15),
            max_diff_bytes: 60_000,
            max_diff_tokens: Some(16_000),
            max_diff_tokens_explicit: false,
            model_context_tokens: None,
        }
    }
}
