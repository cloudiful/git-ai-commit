use crate::config::load_config;

use super::support::{EnvVarGuard, env_lock};

#[test]
fn defaults_confirm_commit_to_true_and_open_editor_to_false() {
    let _guard = env_lock().lock().unwrap();
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", Some("https://example.com/v1"));
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", Some("token"));
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", Some("gpt-4.1-mini"));
    let _confirm_commit = EnvVarGuard::set("GIT_AI_COMMIT_CONFIRM_COMMIT", None);
    let _open_editor = EnvVarGuard::set("GIT_AI_COMMIT_OPEN_EDITOR", None);
    let _config_path = EnvVarGuard::set("GIT_AI_COMMIT_CONFIG_PATH", None);
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let cfg = load_config().expect("expected config");
    assert!(cfg.confirm_commit);
    assert!(!cfg.open_editor);
    assert_eq!(cfg.max_diff_tokens, Some(16_000));
    assert_eq!(cfg.model_context_tokens, None);
}

#[test]
fn reads_token_budget_from_config_file() {
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
  "max_diff_tokens": 4096,
  "model_context_tokens": 8192
}"#,
    )
    .expect("write config");

    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", None);
    let _max_diff_tokens = EnvVarGuard::set("GIT_AI_COMMIT_MAX_DIFF_TOKENS", None);
    let _model_context_tokens = EnvVarGuard::set("GIT_AI_COMMIT_MODEL_CONTEXT_TOKENS", None);
    let _config_path = EnvVarGuard::set(
        "GIT_AI_COMMIT_CONFIG_PATH",
        Some(temp.path().to_str().expect("config path")),
    );
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let cfg = load_config().expect("expected config");

    assert_eq!(cfg.max_diff_tokens, Some(4096));
    assert_eq!(cfg.model_context_tokens, Some(8192));
}

#[test]
fn reads_open_editor_from_config_file_and_env_override() {
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
  "confirm_commit": false,
  "open_editor": true
}"#,
    )
    .expect("write config");

    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", None);
    let _confirm_commit = EnvVarGuard::set("GIT_AI_COMMIT_CONFIRM_COMMIT", None);
    let _open_editor = EnvVarGuard::set("GIT_AI_COMMIT_OPEN_EDITOR", None);
    let _config_path = EnvVarGuard::set(
        "GIT_AI_COMMIT_CONFIG_PATH",
        Some(temp.path().to_str().expect("config path")),
    );
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let from_file = load_config().expect("expected config from file");
    unsafe { std::env::set_var("GIT_AI_COMMIT_OPEN_EDITOR", "false") };
    unsafe { std::env::set_var("GIT_AI_COMMIT_CONFIRM_COMMIT", "true") };
    let from_env = load_config().expect("expected config from env");

    assert!(!from_file.confirm_commit);
    assert!(from_file.open_editor);
    assert!(from_env.confirm_commit);
    assert!(!from_env.open_editor);
}
