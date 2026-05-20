use config::{ConfigSource, ReadOptions, read};
use std::io::{self, ErrorKind};
use std::path::PathBuf;
use std::process::Command;

use super::{FileConfig, RawConfigValues};

#[derive(Clone, Debug, Default)]
pub(super) struct ConfigSnapshot {
    pub(super) env: RawConfigValues,
    pub(super) git: RawConfigValues,
    pub(super) file: Option<FileConfig>,
}

pub(super) fn load_config_snapshot() -> Result<ConfigSnapshot, String> {
    Ok(ConfigSnapshot {
        env: load_env_values(),
        git: load_git_values(),
        file: load_optional_file_config()?,
    })
}

fn load_env_values() -> RawConfigValues {
    RawConfigValues {
        provider: env_value("GIT_AI_COMMIT_PROVIDER"),
        api_base: env_value("GIT_AI_COMMIT_API_BASE"),
        api_key: env_value("GIT_AI_COMMIT_API_KEY"),
        model: env_value("GIT_AI_COMMIT_MODEL"),
        confirm_commit: env_value("GIT_AI_COMMIT_CONFIRM_COMMIT"),
        open_editor: env_value("GIT_AI_COMMIT_OPEN_EDITOR"),
        redact_secrets: env_value("GIT_AI_COMMIT_REDACT_SECRETS"),
        redaction_secret: env_value("GIT_AI_COMMIT_REDACTION_SECRET"),
        redaction_domain: env_value("GIT_AI_COMMIT_REDACTION_DOMAIN"),
        redaction_url: env_value("GIT_AI_COMMIT_REDACTION_URL"),
        redaction_email: env_value("GIT_AI_COMMIT_REDACTION_EMAIL"),
        redaction_ip: env_value("GIT_AI_COMMIT_REDACTION_IP"),
        redaction_cidr: env_value("GIT_AI_COMMIT_REDACTION_CIDR"),
        redaction_phone: env_value("GIT_AI_COMMIT_REDACTION_PHONE"),
        redaction_person: env_value("GIT_AI_COMMIT_REDACTION_PERSON"),
        redaction_organization: env_value("GIT_AI_COMMIT_REDACTION_ORGANIZATION"),
        show_timing: env_value("GIT_AI_COMMIT_SHOW_TIMING"),
        use_env_proxy: env_value("GIT_AI_COMMIT_USE_ENV_PROXY"),
        timeout_sec: env_value("GIT_AI_COMMIT_TIMEOUT_SEC"),
        max_diff_bytes: env_value("GIT_AI_COMMIT_MAX_DIFF_BYTES"),
        max_diff_tokens: env_value("GIT_AI_COMMIT_MAX_DIFF_TOKENS"),
        model_context_tokens: env_value("GIT_AI_COMMIT_MODEL_CONTEXT_TOKENS"),
    }
}

fn load_git_values() -> RawConfigValues {
    RawConfigValues {
        provider: git_value("ai.commit.provider"),
        api_base: git_value("ai.commit.apiBase"),
        api_key: git_value("ai.commit.apiKey"),
        model: git_value("ai.commit.model"),
        confirm_commit: git_value("ai.commit.confirmCommit"),
        open_editor: git_value("ai.commit.openEditor"),
        redact_secrets: git_value("ai.commit.redactSecrets"),
        redaction_secret: git_value("ai.commit.redaction.secret"),
        redaction_domain: git_value("ai.commit.redaction.domain"),
        redaction_url: git_value("ai.commit.redaction.url"),
        redaction_email: git_value("ai.commit.redaction.email"),
        redaction_ip: git_value("ai.commit.redaction.ip"),
        redaction_cidr: git_value("ai.commit.redaction.cidr"),
        redaction_phone: git_value("ai.commit.redaction.phone"),
        redaction_person: git_value("ai.commit.redaction.person"),
        redaction_organization: git_value("ai.commit.redaction.organization"),
        show_timing: git_value("ai.commit.showTiming"),
        use_env_proxy: git_value("ai.commit.useEnvProxy"),
        timeout_sec: git_value("ai.commit.timeoutSec"),
        max_diff_bytes: git_value("ai.commit.maxDiffBytes"),
        max_diff_tokens: git_value("ai.commit.maxDiffTokens"),
        model_context_tokens: git_value("ai.commit.modelContextTokens"),
    }
}

fn env_value(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(non_empty_trimmed)
}

fn git_value(key: &str) -> Option<String> {
    git_config_get(key).ok().and_then(non_empty_trimmed)
}

fn non_empty_trimmed(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(super) fn parse_git_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

pub(super) fn parse_positive_usize(raw: &str) -> Option<usize> {
    match raw.trim().parse::<usize>() {
        Ok(value) if value > 0 => Some(value),
        _ => None,
    }
}

fn load_optional_file_config() -> Result<Option<FileConfig>, String> {
    let Some(path) = std::env::var("GIT_AI_COMMIT_CONFIG_PATH")
        .ok()
        .and_then(|value| non_empty_trimmed(value).map(PathBuf::from))
    else {
        return Ok(None);
    };

    let metadata = std::fs::metadata(&path)
        .map_err(|err| format!("failed to read config file {}: {err}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "failed to read config file {}: path is not a regular file",
            path.display()
        ));
    }

    read(
        &mut PathConfigSource::new(path.clone()),
        Some(ReadOptions::default().without_dotenv()),
    )
    .map(Some)
    .map_err(|err| format!("failed to read config file {}: {err}", path.display()))
}

struct PathConfigSource {
    path: PathBuf,
}

impl PathConfigSource {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ConfigSource for PathConfigSource {
    fn source_name(&self) -> String {
        self.path.display().to_string()
    }

    fn read_value(&mut self) -> io::Result<Option<serde_json::Value>> {
        read_config_value(&self.path).map(Some)
    }

    fn write_config<T>(&mut self, _config: &T) -> io::Result<()>
    where
        T: serde::Serialize,
    {
        Err(io::Error::new(
            ErrorKind::Unsupported,
            format!(
                "writing config through explicit path source is not supported: {}",
                self.path.display()
            ),
        ))
    }
}

fn read_config_value(path: &std::path::Path) -> io::Result<serde_json::Value> {
    let content = std::fs::read_to_string(path).map_err(|err| {
        io::Error::new(
            err.kind(),
            format!("failed to read config {}: {err}", path.display()),
        )
    })?;

    match path.extension().and_then(|suffix| suffix.to_str()) {
        Some("toml") => {
            let toml_value: toml::Value = toml::from_str(&content).map_err(|err| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!("failed to parse TOML config {}: {err}", path.display()),
                )
            })?;

            serde_json::to_value(toml_value).map_err(|err| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "failed to convert TOML config {} to JSON value: {err}",
                        path.display()
                    ),
                )
            })
        }
        Some("json") => serde_json::from_str(&content).map_err(|err| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("failed to parse JSON config {}: {err}", path.display()),
            )
        }),
        Some("jsonc") => serde_json::from_str(&strip_jsonc_comments(&content)).map_err(|err| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("failed to parse JSONC config {}: {err}", path.display()),
            )
        }),
        _ => Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!(
                "config file type not supported for {} (expected .toml, .json, or .jsonc)",
                path.display()
            ),
        )),
    }
}

fn strip_jsonc_comments(content: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Normal,
        InString,
        Escaped,
        LineComment,
        BlockComment,
    }

    let mut output = String::with_capacity(content.len());
    let mut state = State::Normal;
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        match state {
            State::Normal => {
                if ch == '"' {
                    output.push(ch);
                    state = State::InString;
                } else if ch == '/' && matches!(chars.peek(), Some('/')) {
                    output.push(' ');
                    output.push(' ');
                    chars.next();
                    state = State::LineComment;
                } else if ch == '/' && matches!(chars.peek(), Some('*')) {
                    output.push(' ');
                    output.push(' ');
                    chars.next();
                    state = State::BlockComment;
                } else {
                    output.push(ch);
                }
            }
            State::InString => {
                output.push(ch);
                if ch == '\\' {
                    state = State::Escaped;
                } else if ch == '"' {
                    state = State::Normal;
                }
            }
            State::Escaped => {
                output.push(ch);
                state = State::InString;
            }
            State::LineComment => {
                if ch == '\n' {
                    output.push('\n');
                    state = State::Normal;
                } else {
                    output.push(' ');
                }
            }
            State::BlockComment => {
                if ch == '*' && matches!(chars.peek(), Some('/')) {
                    output.push(' ');
                    output.push(' ');
                    chars.next();
                    state = State::Normal;
                } else if ch == '\n' {
                    output.push('\n');
                } else {
                    output.push(' ');
                }
            }
        }
    }

    output
}

pub fn git_config_get(key: &str) -> Result<String, String> {
    let mut command = Command::new("git");
    command.args(["config", "--get", key]);

    if let Ok(repo_root) = std::env::var("GIT_AI_COMMIT_REPO_ROOT")
        && !repo_root.trim().is_empty()
    {
        command.current_dir(repo_root.trim());
    }

    let output = command.output().map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
