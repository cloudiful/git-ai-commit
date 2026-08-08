mod file;
mod git;

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
        git: git::load_git_values(),
        file: file::load_optional_file_config()?,
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
        enable_fallback: env_value("GIT_AI_COMMIT_ENABLE_FALLBACK"),
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
        max_diff_tokens: env_value("GIT_AI_COMMIT_MAX_DIFF_TOKENS"),
        model_context_tokens: env_value("GIT_AI_COMMIT_MODEL_CONTEXT_TOKENS"),
        reasoning_effort: env_value("GIT_AI_COMMIT_REASONING_EFFORT"),
        suppress_diff_dirs: env_value_allow_empty("GIT_AI_COMMIT_SUPPRESS_DIFF_DIRS"),
    }
}

fn env_value(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(non_empty_trimmed)
}

fn env_value_allow_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
}

pub(super) fn non_empty_trimmed(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
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
