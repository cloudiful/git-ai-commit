use crate::config::load_config;

use super::support::TestConfigEnv;

#[test]
fn accepts_git_style_boolean_values() {
    let mut env = TestConfigEnv::new();
    env.set_required_openai_env();
    env.set_env("GIT_AI_COMMIT_CONFIRM_COMMIT", Some("yes"));
    env.set_env("GIT_AI_COMMIT_OPEN_EDITOR", Some("yes"));
    env.set_env("GIT_AI_COMMIT_REDACT_SECRETS", Some("on"));
    env.set_env("GIT_AI_COMMIT_SHOW_TIMING", Some("1"));
    env.set_env("GIT_AI_COMMIT_USE_ENV_PROXY", Some("on"));

    let cfg = load_config().expect("expected config");

    assert!(cfg.confirm_commit);
    assert!(cfg.open_editor);
    assert!(cfg.redact_secrets);
    assert!(cfg.show_timing);
    assert!(cfg.use_env_proxy);
}

#[test]
fn accepts_false_git_style_boolean_values_from_git_config() {
    let env = TestConfigEnv::new();
    env.write_git_config("ai.commit.apiBase", "https://example.com/v1");
    env.write_git_config("ai.commit.apiKey", "token");
    env.write_git_config("ai.commit.model", "gpt-4.1-mini");
    env.write_git_config("ai.commit.confirmCommit", "no");
    env.write_git_config("ai.commit.openEditor", "no");
    env.write_git_config("ai.commit.redactSecrets", "off");
    env.write_git_config("ai.commit.showTiming", "0");
    env.write_git_config("ai.commit.useEnvProxy", "off");

    let cfg = load_config().expect("expected config");

    assert!(!cfg.confirm_commit);
    assert!(!cfg.open_editor);
    assert!(!cfg.redact_secrets);
    assert!(!cfg.show_timing);
    assert!(!cfg.use_env_proxy);
}

#[test]
fn accepts_redaction_rule_booleans_from_env_and_git_config() {
    let mut env = TestConfigEnv::new();
    env.set_required_openai_env();
    env.write_git_config("ai.commit.redaction.domain", "off");
    env.write_git_config("ai.commit.redaction.person", "on");
    env.set_env("GIT_AI_COMMIT_REDACTION_PERSON", Some("false"));

    let cfg = load_config().expect("expected config");

    assert!(!cfg.redaction_rules.domain);
    assert!(!cfg.redaction_rules.person);
}

#[test]
fn rejects_invalid_boolean_value() {
    let mut env = TestConfigEnv::new();
    env.set_required_openai_env();
    env.set_env("GIT_AI_COMMIT_CONFIRM_COMMIT", Some("maybe"));
    env.set_env("GIT_AI_COMMIT_OPEN_EDITOR", Some("maybe"));

    let err = load_config().expect_err("expected invalid bool error");
    assert!(err.contains("invalid ai.commit.confirmCommit value"));
}

#[test]
fn rejects_invalid_redaction_rule_boolean_value() {
    let mut env = TestConfigEnv::new();
    env.set_required_openai_env();
    env.set_env("GIT_AI_COMMIT_REDACTION_DOMAIN", Some("maybe"));

    let err = load_config().expect_err("expected invalid bool error");
    assert!(err.contains("invalid ai.commit.redaction.domain value"));
}
