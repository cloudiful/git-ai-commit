use super::setup::prompt_for_missing_config_with;
use crate::config::{Config, DEFAULT_OLLAMA_API_BASE, Provider};
use std::io::Cursor;
use std::time::Duration;

#[test]
fn canceling_at_api_base_does_not_write_anything() {
    let existing = sample_config(Provider::Ollama, DEFAULT_OLLAMA_API_BASE, "", "");
    let mut input = Cursor::new(b"openai-compatible\n\n".as_slice());
    let mut output = Vec::new();
    let mut writes = Vec::new();
    let error = prompt_for_missing_config_with(&existing, &mut input, &mut output, |key, value| {
        writes.push((key.to_string(), value.to_string()));
        Ok(())
    })
    .expect_err("expected setup cancellation");
    assert_eq!(error, "setup canceled");
    assert!(writes.is_empty());
}

#[test]
fn canceling_at_api_key_does_not_write_anything() {
    let existing = sample_config(Provider::OpenAiCompatible, "", "", "");
    let mut input = Cursor::new(b"\nhttps://example.com/v1\n\n".as_slice());
    let mut output = Vec::new();
    let mut writes = Vec::new();
    let error = prompt_for_missing_config_with(&existing, &mut input, &mut output, |key, value| {
        writes.push((key.to_string(), value.to_string()));
        Ok(())
    })
    .expect_err("expected setup cancellation");
    assert_eq!(error, "setup canceled");
    assert!(writes.is_empty());
}

#[test]
fn writes_all_required_openai_fields_after_collecting_everything() {
    let cases = [
        (
            sample_config(Provider::OpenAiCompatible, "", "", ""),
            b"\nhttps://example.com/v1\nsecret-token\ngpt-4.1-mini\n".as_slice(),
            vec![
                (
                    "ai.commit.provider".to_string(),
                    "openai-compatible".to_string(),
                ),
                (
                    "ai.commit.apiBase".to_string(),
                    "https://example.com/v1".to_string(),
                ),
                ("ai.commit.apiKey".to_string(), "secret-token".to_string()),
                ("ai.commit.model".to_string(), "gpt-4.1-mini".to_string()),
            ],
        ),
        (
            sample_config(
                Provider::OpenAiCompatible,
                "https://api.openai.com/v1",
                "secret-token",
                "gpt-4.1-mini",
            ),
            b"ollama\n\nllama3.2\n".as_slice(),
            vec![
                ("ai.commit.provider".to_string(), "ollama".to_string()),
                (
                    "ai.commit.apiBase".to_string(),
                    DEFAULT_OLLAMA_API_BASE.to_string(),
                ),
                ("ai.commit.model".to_string(), "llama3.2".to_string()),
            ],
        ),
        (
            sample_config(
                Provider::OpenAiCompatible,
                "https://api.openai.com/v1",
                "secret-token",
                "gpt-4.1-mini",
            ),
            b"ollama\nhttp://10.0.0.5:11434\nqwen3:8b\n".as_slice(),
            vec![
                ("ai.commit.provider".to_string(), "ollama".to_string()),
                (
                    "ai.commit.apiBase".to_string(),
                    "http://10.0.0.5:11434".to_string(),
                ),
                ("ai.commit.model".to_string(), "qwen3:8b".to_string()),
            ],
        ),
    ];
    for (existing, bytes, expected) in cases {
        let mut input = Cursor::new(bytes);
        let mut output = Vec::new();
        let mut writes = Vec::new();
        prompt_for_missing_config_with(&existing, &mut input, &mut output, |key, value| {
            writes.push((key.to_string(), value.to_string()));
            Ok(())
        })
        .expect("expected setup to succeed");
        assert_eq!(writes, expected);
    }
}

fn sample_config(provider: Provider, api_base: &str, api_key: &str, model: &str) -> Config {
    Config {
        provider,
        api_base: api_base.to_string(),
        api_key: api_key.to_string(),
        model: model.to_string(),
        confirm_commit: true,
        open_editor: false,
        enable_fallback: false,
        redact_secrets: true,
        redaction_rules: crate::config::default_redaction_rules(),
        show_timing: true,
        use_env_proxy: false,
        timeout: Duration::from_secs(5),
        max_diff_tokens: 16_000,
        max_diff_tokens_explicit: false,
        model_context_tokens: None,
        reasoning_effort: crate::config::ReasoningEffort::Low,
    }
}
