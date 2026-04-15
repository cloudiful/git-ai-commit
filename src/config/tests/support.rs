use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

const CONFIG_ENV_KEYS: [&str; 14] = [
    "GIT_AI_COMMIT_PROVIDER",
    "GIT_AI_COMMIT_API_BASE",
    "GIT_AI_COMMIT_API_KEY",
    "GIT_AI_COMMIT_MODEL",
    "GIT_AI_COMMIT_CONFIRM_COMMIT",
    "GIT_AI_COMMIT_OPEN_EDITOR",
    "GIT_AI_COMMIT_REDACT_SECRETS",
    "GIT_AI_COMMIT_SHOW_TIMING",
    "GIT_AI_COMMIT_USE_ENV_PROXY",
    "GIT_AI_COMMIT_TIMEOUT_SEC",
    "GIT_AI_COMMIT_MAX_DIFF_BYTES",
    "GIT_AI_COMMIT_MAX_DIFF_TOKENS",
    "GIT_AI_COMMIT_MODEL_CONTEXT_TOKENS",
    "GIT_AI_COMMIT_CONFIG_PATH",
];

pub(super) fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(super) struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    pub(super) fn set(key: &'static str, value: Option<&str>) -> Self {
        let previous = std::env::var(key).ok();
        match value {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

pub(super) struct TestConfigEnv {
    _lock: MutexGuard<'static, ()>,
    guards: Vec<EnvVarGuard>,
    managed_keys: Vec<&'static str>,
    git_global: tempfile::NamedTempFile,
    config_file: Option<tempfile::NamedTempFile>,
    temp_dir: Option<tempfile::TempDir>,
}

impl TestConfigEnv {
    pub(super) fn new() -> Self {
        let lock = env_lock().lock().unwrap();
        let git_global = tempfile::NamedTempFile::new().expect("git global");
        let mut env = Self {
            _lock: lock,
            guards: Vec::new(),
            managed_keys: Vec::new(),
            git_global,
            config_file: None,
            temp_dir: None,
        };
        let git_global_path = env
            .git_global
            .path()
            .to_str()
            .expect("git global path")
            .to_string();

        for key in CONFIG_ENV_KEYS {
            env.set_env(key, None);
        }
        env.set_env("GIT_CONFIG_GLOBAL", Some(&git_global_path));
        env.set_env("GIT_CONFIG_NOSYSTEM", Some("1"));
        env
    }

    pub(super) fn set_required_openai_env(&mut self) {
        self.set_env("GIT_AI_COMMIT_PROVIDER", None);
        self.set_env("GIT_AI_COMMIT_API_BASE", Some("https://example.com/v1"));
        self.set_env("GIT_AI_COMMIT_API_KEY", Some("token"));
        self.set_env("GIT_AI_COMMIT_MODEL", Some("gpt-4.1-mini"));
    }

    pub(super) fn set_env(&mut self, key: &'static str, value: Option<&str>) {
        if self.managed_keys.contains(&key) {
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
            return;
        }

        self.guards.push(EnvVarGuard::set(key, value));
        self.managed_keys.push(key);
    }

    pub(super) fn write_git_config(&self, key: &str, value: &str) {
        write_git_config(self.git_global.path(), key, value);
    }

    pub(super) fn write_config_file(&mut self, contents: &str) {
        let file = tempfile::Builder::new()
            .suffix(".json")
            .tempfile()
            .expect("temp config");
        std::fs::write(file.path(), contents).expect("write config");
        self.set_env(
            "GIT_AI_COMMIT_CONFIG_PATH",
            Some(file.path().to_str().expect("config path")),
        );
        self.config_file = Some(file);
    }

    pub(super) fn set_missing_config_path(&mut self) {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path = temp_dir.path().join("missing-config.json");
        self.set_env(
            "GIT_AI_COMMIT_CONFIG_PATH",
            Some(missing_path.to_str().expect("missing config path")),
        );
        self.temp_dir = Some(temp_dir);
    }
}

pub(super) fn write_git_config(path: &std::path::Path, key: &str, value: &str) {
    let status = Command::new("git")
        .args([
            "config",
            "--file",
            path.to_str().expect("git config path"),
            key,
            value,
        ])
        .status()
        .expect("git config command");
    assert!(status.success());
}
