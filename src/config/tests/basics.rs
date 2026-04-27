use crate::config::load_config;

use super::support::TestConfigEnv;

#[test]
fn defaults_confirm_commit_to_true_and_open_editor_to_false() {
    let mut env = TestConfigEnv::new();
    env.set_required_openai_env();

    let cfg = load_config().expect("expected config");
    assert!(cfg.confirm_commit);
    assert!(!cfg.open_editor);
    assert_eq!(cfg.max_diff_tokens, Some(16_000));
    assert_eq!(cfg.model_context_tokens, None);
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

    assert_eq!(cfg.max_diff_tokens, Some(4096));
    assert_eq!(cfg.model_context_tokens, Some(8192));
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
  "open_editor": true
}"#,
    );

    let from_file = load_config().expect("expected config from file");
    env.set_env("GIT_AI_COMMIT_OPEN_EDITOR", Some("false"));
    env.set_env("GIT_AI_COMMIT_CONFIRM_COMMIT", Some("true"));
    let from_env = load_config().expect("expected config from env");

    assert!(!from_file.confirm_commit);
    assert!(from_file.open_editor);
    assert!(from_env.confirm_commit);
    assert!(!from_env.open_editor);
}
