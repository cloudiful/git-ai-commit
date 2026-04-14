use crate::config::load_partial_config;

use super::support::{EnvVarGuard, env_lock};

#[test]
fn surfaces_missing_config_file_path_errors() {
    let _guard = env_lock().lock().unwrap();
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let missing_path = temp_dir.path().join("missing-config.json");
    let _config_path = EnvVarGuard::set(
        "GIT_AI_COMMIT_CONFIG_PATH",
        Some(missing_path.to_str().expect("missing config path")),
    );
    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let err = load_partial_config().expect_err("expected file read error");
    assert!(err.contains("failed to read config file"));
}

#[test]
fn surfaces_invalid_config_file_contents() {
    let _guard = env_lock().lock().unwrap();
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    let temp = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp config");
    std::fs::write(temp.path(), "{ invalid json").expect("write invalid config");

    let _config_path = EnvVarGuard::set(
        "GIT_AI_COMMIT_CONFIG_PATH",
        Some(temp.path().to_str().expect("config path")),
    );
    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let err = load_partial_config().expect_err("expected invalid file error");
    assert!(err.contains("failed to read config file"));
}

#[test]
fn surfaces_unreadable_config_file_targets() {
    let _guard = env_lock().lock().unwrap();
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    let temp_dir = tempfile::tempdir().expect("temp dir");

    let _config_path = EnvVarGuard::set(
        "GIT_AI_COMMIT_CONFIG_PATH",
        Some(temp_dir.path().to_str().expect("config dir path")),
    );
    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let err = load_partial_config().expect_err("expected unreadable file error");
    assert!(err.contains("failed to read config file"));
}

#[test]
fn rejects_invalid_token_budget_values() {
    let _guard = env_lock().lock().unwrap();
    let temp = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp config");
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    std::fs::write(
        temp.path(),
        r#"{
  "api_base": "https://example.com/v1",
  "api_key": "token",
  "model": "gpt-4.1-mini",
  "max_diff_tokens": 0
}"#,
    )
    .expect("write invalid config");

    let _config_path = EnvVarGuard::set(
        "GIT_AI_COMMIT_CONFIG_PATH",
        Some(temp.path().to_str().expect("config path")),
    );
    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let err = load_partial_config().expect_err("expected invalid token budget");
    assert!(err.contains("invalid ai.commit.maxDiffTokens value"));
}

#[test]
fn reads_redact_secrets_from_file_config() {
    let _guard = env_lock().lock().unwrap();
    let temp = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp config");
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    std::fs::write(
        temp.path(),
        r#"{
  "api_base": "https://example.com/v1",
  "api_key": "token",
  "model": "gpt-4.1-mini",
  "redact_secrets": false
}"#,
    )
    .expect("write config");

    let _config_path = EnvVarGuard::set(
        "GIT_AI_COMMIT_CONFIG_PATH",
        Some(temp.path().to_str().expect("config path")),
    );
    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", None);
    let _redact_secrets = EnvVarGuard::set("GIT_AI_COMMIT_REDACT_SECRETS", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let cfg = load_partial_config().expect("expected config from file");
    assert!(!cfg.redact_secrets);
}
