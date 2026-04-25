use crate::config::{DEFAULT_OLLAMA_API_BASE, Provider, load_config, load_partial_config};

use super::support::TestConfigEnv;

#[test]
fn defaults_provider_to_openai_compatible() {
    let mut env = TestConfigEnv::new();
    env.set_required_openai_env();

    let cfg = load_config().expect("expected config");

    assert_eq!(cfg.provider, Provider::OpenAiCompatible);
}

#[test]
fn reads_provider_with_normal_precedence() {
    let mut env = TestConfigEnv::new();
    env.write_config_file(
        r#"{
  "provider": "openai-compatible",
  "api_base": "https://file.example.com/v1",
  "api_key": "file-token",
  "model": "file-model"
}"#,
    );
    env.write_git_config("ai.commit.provider", "ollama");
    env.write_git_config("ai.commit.model", "git-model");

    env.set_env("GIT_AI_COMMIT_PROVIDER", Some("openai"));
    env.set_env("GIT_AI_COMMIT_API_BASE", Some("https://env.example.com/v1"));
    env.set_env("GIT_AI_COMMIT_API_KEY", Some("env-token"));
    env.set_env("GIT_AI_COMMIT_MODEL", Some("env-model"));

    let cfg = load_config().expect("expected config");

    assert_eq!(cfg.provider, Provider::OpenAiCompatible);
    assert_eq!(cfg.model, "env-model");
}

#[test]
fn defaults_ollama_api_base_and_allows_missing_local_api_key() {
    let mut env = TestConfigEnv::new();
    env.set_env("GIT_AI_COMMIT_PROVIDER", Some("ollama"));
    env.set_env("GIT_AI_COMMIT_MODEL", Some("llama3.2"));

    let cfg = load_config().expect("expected ollama config");

    assert_eq!(cfg.provider, Provider::Ollama);
    assert_eq!(cfg.api_base, DEFAULT_OLLAMA_API_BASE);
    assert_eq!(cfg.api_key, "");
    assert!(cfg.is_local_ollama());
}

#[test]
fn requires_api_key_for_ollama_cloud() {
    let mut env = TestConfigEnv::new();
    env.set_env("GIT_AI_COMMIT_PROVIDER", Some("ollama"));
    env.set_env("GIT_AI_COMMIT_API_BASE", Some("https://ollama.com/v1"));
    env.set_env("GIT_AI_COMMIT_MODEL", Some("gpt-oss:20b"));

    let err = load_config().expect_err("expected missing api key");

    assert!(err.contains("ai.commit.apiKey"));
}

#[test]
fn allows_missing_api_key_for_custom_remote_ollama() {
    let mut env = TestConfigEnv::new();
    env.set_env("GIT_AI_COMMIT_PROVIDER", Some("ollama"));
    env.set_env("GIT_AI_COMMIT_API_BASE", Some("http://10.0.0.5:11434"));
    env.set_env("GIT_AI_COMMIT_MODEL", Some("llama3.2"));

    let cfg = load_config().expect("expected remote ollama config");

    assert_eq!(cfg.api_key, "");
    assert!(!cfg.requires_api_key());
}

#[test]
fn rejects_invalid_provider_values() {
    let mut env = TestConfigEnv::new();
    env.set_required_openai_env();
    env.set_env("GIT_AI_COMMIT_PROVIDER", Some("wat"));

    let err = load_partial_config().expect_err("expected invalid provider");

    assert!(err.contains("invalid ai.commit.provider value"));
}

#[test]
fn auto_detects_context_only_for_openrouter_without_explicit_value() {
    let mut env = TestConfigEnv::new();
    env.set_env("GIT_AI_COMMIT_PROVIDER", Some("openai-compatible"));
    env.set_env(
        "GIT_AI_COMMIT_API_BASE",
        Some("https://openrouter.ai/api/v1"),
    );
    env.set_env("GIT_AI_COMMIT_API_KEY", Some("token"));
    env.set_env("GIT_AI_COMMIT_MODEL", Some("google/gemma-4-31b-it:free"));

    let cfg = load_config().expect("expected config");
    assert!(cfg.should_auto_detect_model_context_tokens());

    env.set_env("GIT_AI_COMMIT_MODEL_CONTEXT_TOKENS", Some("32768"));
    let cfg = load_config().expect("expected config with explicit context");
    assert!(!cfg.should_auto_detect_model_context_tokens());
}
