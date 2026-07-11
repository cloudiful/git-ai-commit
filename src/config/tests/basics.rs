use crate::config::load_config;

use super::support::TestConfigEnv;

#[test]
fn defaults_confirm_commit_to_true_and_open_editor_to_false() {
    let mut env = TestConfigEnv::new();
    env.set_required_openai_env();

    let cfg = load_config().expect("expected config");
    assert!(cfg.confirm_commit);
    assert!(!cfg.open_editor);
    assert!(!cfg.enable_fallback);
    assert_eq!(cfg.max_diff_tokens, 32_000);
    assert_eq!(cfg.model_context_tokens, None);
    assert_eq!(cfg.reasoning_effort.as_api_value(), "low");
    assert!(cfg.redaction_rules.domain);
    assert!(!cfg.redaction_rules.person);
}

#[test]
fn reads_token_budget_from_config_file() {
    let mut env = TestConfigEnv::new();
    env.write_config_file(
        r#"{
  "api_base": "https://example.com/v1",
  "api_key": "token",
  "model": "gpt-4.1-mini",
  "max_diff_tokens": 4096,
  "model_context_tokens": 8192
}"#,
    );

    let cfg = load_config().expect("expected config");

    assert_eq!(cfg.max_diff_tokens, 4096);
    assert_eq!(cfg.model_context_tokens, Some(8192));
}

#[test]
fn reads_reasoning_effort_from_config_file() {
    let mut env = TestConfigEnv::new();
    env.write_config_file(
        r#"{
  "api_base": "https://example.com/v1",
  "api_key": "token",
  "model": "gpt-4.1-mini",
  "reasoning_effort": "high"
}"#,
    );

    let cfg = load_config().expect("expected config");

    assert_eq!(cfg.reasoning_effort.as_api_value(), "high");
}

#[test]
fn reads_reasoning_effort_from_git_config() {
    let env = TestConfigEnv::new();
    env.write_git_config("ai.commit.apiBase", "https://example.com/v1");
    env.write_git_config("ai.commit.apiKey", "token");
    env.write_git_config("ai.commit.model", "gpt-4.1-mini");
    env.write_git_config("ai.commit.reasoningEffort", "medium");

    let cfg = load_config().expect("expected config");

    assert_eq!(cfg.reasoning_effort.as_api_value(), "medium");
}

#[test]
fn reads_open_editor_from_config_file_and_env_override() {
    let mut env = TestConfigEnv::new();
    env.write_config_file(
        r#"{
  "api_base": "https://example.com/v1",
  "api_key": "token",
  "model": "gpt-4.1-mini",
  "confirm_commit": false,
  "open_editor": true,
  "enable_fallback": true
}"#,
    );

    let from_file = load_config().expect("expected config from file");
    env.set_env("GIT_AI_COMMIT_OPEN_EDITOR", Some("false"));
    env.set_env("GIT_AI_COMMIT_CONFIRM_COMMIT", Some("true"));
    env.set_env("GIT_AI_COMMIT_ENABLE_FALLBACK", Some("false"));
    let from_env = load_config().expect("expected config from env");

    assert!(!from_file.confirm_commit);
    assert!(from_file.open_editor);
    assert!(from_file.enable_fallback);
    assert!(from_env.confirm_commit);
    assert!(!from_env.open_editor);
    assert!(!from_env.enable_fallback);
}
