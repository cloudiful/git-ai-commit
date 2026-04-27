use crate::config::load_partial_config;

use super::support::TestConfigEnv;

#[test]
fn surfaces_missing_config_file_path_errors() {
    let mut env = TestConfigEnv::new();
    env.set_missing_config_path();

    let err = load_partial_config().expect_err("expected file read error");
    assert!(err.contains("failed to read config file"));
}

#[test]
fn surfaces_invalid_config_file_contents() {
    let mut env = TestConfigEnv::new();
    env.write_config_file("{ invalid json");

    let err = load_partial_config().expect_err("expected invalid file error");
    assert!(err.contains("failed to read config file"));
}

#[test]
fn rejects_invalid_token_budget_values() {
    let mut env = TestConfigEnv::new();
    env.write_config_file(
        r#"{
  "api_base": "https://example.com/v1",
  "api_key": "token",
  "model": "gpt-4.1-mini",
  "max_diff_tokens": 0
}"#,
    );

    let err = load_partial_config().expect_err("expected invalid token budget");
    assert!(err.contains("invalid ai.commit.maxDiffTokens value"));
}

#[test]
fn reads_redact_secrets_from_file_config() {
    let mut env = TestConfigEnv::new();
    env.write_config_file(
        r#"{
  "api_base": "https://example.com/v1",
  "api_key": "token",
  "model": "gpt-4.1-mini",
  "redact_secrets": false
}"#,
    );

    let cfg = load_partial_config().expect("expected config from file");
    assert!(!cfg.redact_secrets);
}

#[test]
fn reads_redaction_rules_from_file_config() {
    let mut env = TestConfigEnv::new();
    env.write_config_file(
        r#"{
  "api_base": "https://example.com/v1",
  "api_key": "token",
  "model": "gpt-4.1-mini",
  "redaction_rules": {
    "domain": false,
    "person": true
  }
}"#,
    );

    let cfg = load_partial_config().expect("expected config from file");
    assert!(!cfg.redaction_rules.domain);
    assert!(cfg.redaction_rules.person);
    assert!(cfg.redaction_rules.secret);
}
