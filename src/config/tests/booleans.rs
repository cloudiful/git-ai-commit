use crate::config::load_config;

use super::support::{EnvVarGuard, env_lock, write_git_config};

#[test]
fn accepts_git_style_boolean_values() {
    let _guard = env_lock().lock().unwrap();
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", Some("https://example.com/v1"));
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", Some("token"));
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", Some("gpt-4.1-mini"));
    let _confirm_commit = EnvVarGuard::set("GIT_AI_COMMIT_CONFIRM_COMMIT", Some("yes"));
    let _open_editor = EnvVarGuard::set("GIT_AI_COMMIT_OPEN_EDITOR", Some("yes"));
    let _redact_secrets = EnvVarGuard::set("GIT_AI_COMMIT_REDACT_SECRETS", Some("on"));
    let _show_timing = EnvVarGuard::set("GIT_AI_COMMIT_SHOW_TIMING", Some("1"));
    let _use_env_proxy = EnvVarGuard::set("GIT_AI_COMMIT_USE_ENV_PROXY", Some("on"));
    let _config_path = EnvVarGuard::set("GIT_AI_COMMIT_CONFIG_PATH", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let cfg = load_config().expect("expected config");

    assert!(cfg.confirm_commit);
    assert!(cfg.open_editor);
    assert!(cfg.redact_secrets);
    assert!(cfg.show_timing);
    assert!(cfg.use_env_proxy);
}

#[test]
fn accepts_false_git_style_boolean_values_from_git_config() {
    let _guard = env_lock().lock().unwrap();
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    write_git_config(
        git_global.path(),
        "ai.commit.apiBase",
        "https://example.com/v1",
    );
    write_git_config(git_global.path(), "ai.commit.apiKey", "token");
    write_git_config(git_global.path(), "ai.commit.model", "gpt-4.1-mini");
    write_git_config(git_global.path(), "ai.commit.confirmCommit", "no");
    write_git_config(git_global.path(), "ai.commit.openEditor", "no");
    write_git_config(git_global.path(), "ai.commit.redactSecrets", "off");
    write_git_config(git_global.path(), "ai.commit.showTiming", "0");
    write_git_config(git_global.path(), "ai.commit.useEnvProxy", "off");

    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", None);
    let _confirm_commit = EnvVarGuard::set("GIT_AI_COMMIT_CONFIRM_COMMIT", None);
    let _open_editor = EnvVarGuard::set("GIT_AI_COMMIT_OPEN_EDITOR", None);
    let _redact_secrets = EnvVarGuard::set("GIT_AI_COMMIT_REDACT_SECRETS", None);
    let _show_timing = EnvVarGuard::set("GIT_AI_COMMIT_SHOW_TIMING", None);
    let _use_env_proxy = EnvVarGuard::set("GIT_AI_COMMIT_USE_ENV_PROXY", None);
    let _config_path = EnvVarGuard::set("GIT_AI_COMMIT_CONFIG_PATH", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let cfg = load_config().expect("expected config");

    assert!(!cfg.confirm_commit);
    assert!(!cfg.open_editor);
    assert!(!cfg.redact_secrets);
    assert!(!cfg.show_timing);
    assert!(!cfg.use_env_proxy);
}

#[test]
fn rejects_invalid_boolean_value() {
    let _guard = env_lock().lock().unwrap();
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", Some("https://example.com/v1"));
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", Some("token"));
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", Some("gpt-4.1-mini"));
    let _confirm_commit = EnvVarGuard::set("GIT_AI_COMMIT_CONFIRM_COMMIT", Some("maybe"));
    let _open_editor = EnvVarGuard::set("GIT_AI_COMMIT_OPEN_EDITOR", Some("maybe"));
    let _redact_secrets = EnvVarGuard::set("GIT_AI_COMMIT_REDACT_SECRETS", None);
    let _config_path = EnvVarGuard::set("GIT_AI_COMMIT_CONFIG_PATH", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let err = load_config().expect_err("expected invalid bool error");
    assert!(err.contains("invalid ai.commit.confirmCommit value"));
}
