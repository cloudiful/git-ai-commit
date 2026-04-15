use crate::config::load_config;

use super::support::TestConfigEnv;

#[test]
fn preserves_env_git_file_precedence() {
    let mut env = TestConfigEnv::new();
    env.write_config_file(
        r#"{
  "api_base": "https://file.example.com/v1",
  "api_key": "file-token",
  "model": "file-model",
  "confirm_commit": false,
  "open_editor": false
}"#,
    );
    env.write_git_config("ai.commit.apiBase", "https://git.example.com/v1");
    env.write_git_config("ai.commit.apiKey", "git-token");
    env.write_git_config("ai.commit.model", "git-model");
    env.write_git_config("ai.commit.confirmCommit", "no");
    env.write_git_config("ai.commit.openEditor", "yes");
    env.write_git_config("ai.commit.maxDiffTokens", "2000");
    env.write_git_config("ai.commit.modelContextTokens", "4000");

    env.set_env("GIT_AI_COMMIT_MODEL", Some("env-model"));
    env.set_env("GIT_AI_COMMIT_CONFIRM_COMMIT", Some("yes"));
    env.set_env("GIT_AI_COMMIT_MAX_DIFF_TOKENS", Some("3000"));

    let cfg = load_config().expect("expected config");

    assert_eq!(cfg.api_base, "https://git.example.com/v1");
    assert_eq!(cfg.api_key, "git-token");
    assert_eq!(cfg.model, "env-model");
    assert!(cfg.confirm_commit);
    assert!(cfg.open_editor);
    assert_eq!(cfg.max_diff_tokens, Some(3000));
    assert_eq!(cfg.model_context_tokens, Some(4000));
}
