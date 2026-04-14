use crate::config::load_config;

use super::support::{EnvVarGuard, env_lock, write_git_config};

#[test]
fn preserves_env_git_file_precedence() {
    let _guard = env_lock().lock().unwrap();
    let file = tempfile::Builder::new()
        .suffix(".json")
        .tempfile()
        .expect("temp config");
    std::fs::write(
        file.path(),
        r#"{
  "api_base": "https://file.example.com/v1",
  "api_key": "file-token",
  "model": "file-model",
  "confirm_commit": false,
  "open_editor": false
}"#,
    )
    .expect("write config");
    let git_global = tempfile::NamedTempFile::new().expect("git global");
    write_git_config(
        git_global.path(),
        "ai.commit.apiBase",
        "https://git.example.com/v1",
    );
    write_git_config(git_global.path(), "ai.commit.apiKey", "git-token");
    write_git_config(git_global.path(), "ai.commit.model", "git-model");
    write_git_config(git_global.path(), "ai.commit.confirmCommit", "no");
    write_git_config(git_global.path(), "ai.commit.openEditor", "yes");
    write_git_config(git_global.path(), "ai.commit.maxDiffTokens", "2000");
    write_git_config(git_global.path(), "ai.commit.modelContextTokens", "4000");

    let _api_base = EnvVarGuard::set("GIT_AI_COMMIT_API_BASE", None);
    let _api_key = EnvVarGuard::set("GIT_AI_COMMIT_API_KEY", None);
    let _model = EnvVarGuard::set("GIT_AI_COMMIT_MODEL", Some("env-model"));
    let _confirm_commit = EnvVarGuard::set("GIT_AI_COMMIT_CONFIRM_COMMIT", Some("yes"));
    let _open_editor = EnvVarGuard::set("GIT_AI_COMMIT_OPEN_EDITOR", None);
    let _max_diff_tokens = EnvVarGuard::set("GIT_AI_COMMIT_MAX_DIFF_TOKENS", Some("3000"));
    let _model_context_tokens = EnvVarGuard::set("GIT_AI_COMMIT_MODEL_CONTEXT_TOKENS", None);
    let _config_path = EnvVarGuard::set(
        "GIT_AI_COMMIT_CONFIG_PATH",
        Some(file.path().to_str().expect("config path")),
    );
    let _git_config_global = EnvVarGuard::set(
        "GIT_CONFIG_GLOBAL",
        Some(git_global.path().to_str().expect("git global path")),
    );
    let _git_config_nosystem = EnvVarGuard::set("GIT_CONFIG_NOSYSTEM", Some("1"));

    let cfg = load_config().expect("expected config");

    assert_eq!(cfg.api_base, "https://git.example.com/v1");
    assert_eq!(cfg.api_key, "git-token");
    assert_eq!(cfg.model, "env-model");
    assert!(cfg.confirm_commit);
    assert!(cfg.open_editor);
    assert_eq!(cfg.max_diff_tokens, Some(3000));
    assert_eq!(cfg.model_context_tokens, Some(4000));
}
