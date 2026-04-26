mod provider;
mod sources;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub use self::provider::{
    DEFAULT_OLLAMA_API_BASE, Provider, is_anthropic_compatible_url, is_loopback_url,
    is_ollama_cloud_url, is_openrouter_url,
};
use self::sources::{ConfigSnapshot, load_config_snapshot};

pub const DEFAULT_TIMEOUT_SEC: u64 = 15;
pub const DEFAULT_MAX_DIFF_BYTES: usize = 60_000;
pub const DEFAULT_MAX_DIFF_TOKENS: usize = 16_000;
pub const MAX_AUTO_DIFF_TOKENS: usize = 64_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffBudgetConfig {
    Bytes {
        max_bytes: usize,
    },
    Tokens {
        max_tokens: usize,
        model_context_tokens: Option<usize>,
    },
}

#[derive(Clone, Debug)]
pub struct Config {
    pub provider: Provider,
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub confirm_commit: bool,
    pub open_editor: bool,
    pub redact_secrets: bool,
    pub show_timing: bool,
    pub use_env_proxy: bool,
    pub timeout: Duration,
    pub max_diff_bytes: usize,
    pub max_diff_tokens: Option<usize>,
    pub max_diff_tokens_explicit: bool,
    pub model_context_tokens: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct FileConfig {
    pub(super) provider: Option<String>,
    pub(super) api_base: Option<String>,
    pub(super) api_key: Option<String>,
    pub(super) model: Option<String>,
    pub(super) confirm_commit: Option<bool>,
    pub(super) open_editor: Option<bool>,
    pub(super) redact_secrets: Option<bool>,
    pub(super) show_timing: Option<bool>,
    pub(super) use_env_proxy: Option<bool>,
    pub(super) timeout_sec: Option<usize>,
    pub(super) max_diff_bytes: Option<usize>,
    pub(super) max_diff_tokens: Option<usize>,
    pub(super) model_context_tokens: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RawConfigValues {
    pub(super) provider: Option<String>,
    pub(super) api_base: Option<String>,
    pub(super) api_key: Option<String>,
    pub(super) model: Option<String>,
    pub(super) confirm_commit: Option<String>,
    pub(super) open_editor: Option<String>,
    pub(super) redact_secrets: Option<String>,
    pub(super) show_timing: Option<String>,
    pub(super) use_env_proxy: Option<String>,
    pub(super) timeout_sec: Option<String>,
    pub(super) max_diff_bytes: Option<String>,
    pub(super) max_diff_tokens: Option<String>,
    pub(super) model_context_tokens: Option<String>,
}

pub fn load_config() -> Result<Config, String> {
    let cfg = load_partial_config()?;
    let missing = missing_required_config_keys(&cfg);
    if !missing.is_empty() {
        return Err(format!("missing {}", missing.join(", ")));
    }
    Ok(cfg)
}

pub fn load_partial_config() -> Result<Config, String> {
    let snapshot = load_config_snapshot()?;
    let provider = snapshot.provider_value()?;
    let api_base = snapshot.string_value(
        |values| values.api_base.as_ref(),
        |cfg| cfg.api_base.as_ref(),
    );
    let api_base = if provider == Provider::Ollama && api_base.is_empty() {
        DEFAULT_OLLAMA_API_BASE.to_string()
    } else {
        api_base
    };
    let max_diff_tokens_explicit = snapshot.has_configured_value(
        |values| values.max_diff_tokens.as_ref(),
        |cfg| cfg.max_diff_tokens,
    );
    Ok(Config {
        provider,
        api_base,
        api_key: snapshot
            .string_value(|values| values.api_key.as_ref(), |cfg| cfg.api_key.as_ref()),
        model: snapshot.string_value(|values| values.model.as_ref(), |cfg| cfg.model.as_ref()),
        confirm_commit: snapshot.bool_value(
            "ai.commit.confirmCommit",
            |values| values.confirm_commit.as_ref(),
            |cfg| cfg.confirm_commit,
            true,
        )?,
        open_editor: snapshot.bool_value(
            "ai.commit.openEditor",
            |values| values.open_editor.as_ref(),
            |cfg| cfg.open_editor,
            false,
        )?,
        redact_secrets: snapshot.bool_value(
            "ai.commit.redactSecrets",
            |values| values.redact_secrets.as_ref(),
            |cfg| cfg.redact_secrets,
            true,
        )?,
        show_timing: snapshot.bool_value(
            "ai.commit.showTiming",
            |values| values.show_timing.as_ref(),
            |cfg| cfg.show_timing,
            true,
        )?,
        use_env_proxy: snapshot.bool_value(
            "ai.commit.useEnvProxy",
            |values| values.use_env_proxy.as_ref(),
            |cfg| cfg.use_env_proxy,
            false,
        )?,
        timeout: Duration::from_secs(snapshot.int_value(
            "ai.commit.timeoutSec",
            |values| values.timeout_sec.as_ref(),
            |cfg| cfg.timeout_sec,
            DEFAULT_TIMEOUT_SEC as usize,
        )? as u64),
        max_diff_bytes: snapshot.int_value(
            "ai.commit.maxDiffBytes",
            |values| values.max_diff_bytes.as_ref(),
            |cfg| cfg.max_diff_bytes,
            DEFAULT_MAX_DIFF_BYTES,
        )?,
        max_diff_tokens: Some(snapshot.int_value(
            "ai.commit.maxDiffTokens",
            |values| values.max_diff_tokens.as_ref(),
            |cfg| cfg.max_diff_tokens,
            DEFAULT_MAX_DIFF_TOKENS,
        )?),
        max_diff_tokens_explicit,
        model_context_tokens: snapshot.optional_int_value(
            "ai.commit.modelContextTokens",
            |values| values.model_context_tokens.as_ref(),
            |cfg| cfg.model_context_tokens,
        )?,
    })
}

pub fn missing_required_config_keys(cfg: &Config) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if cfg.api_base.trim().is_empty() {
        missing.push("ai.commit.apiBase");
    }
    if cfg.requires_api_key() && cfg.api_key.trim().is_empty() {
        missing.push("ai.commit.apiKey");
    }
    if cfg.model.trim().is_empty() {
        missing.push("ai.commit.model");
    }
    missing
}

impl ConfigSnapshot {
    pub(super) fn provider_value(&self) -> Result<Provider, String> {
        let raw = self
            .env
            .provider
            .as_ref()
            .or(self.git.provider.as_ref())
            .or_else(|| self.file.as_ref().and_then(|cfg| cfg.provider.as_ref()));

        match raw {
            Some(raw) => Provider::parse(raw)
                .ok_or_else(|| format!("invalid ai.commit.provider value {:?}", raw)),
            None => Ok(Provider::default()),
        }
    }

    pub(super) fn string_value(
        &self,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<&String>,
    ) -> String {
        raw_getter(&self.env)
            .cloned()
            .or_else(|| raw_getter(&self.git).cloned())
            .or_else(|| self.file.as_ref().and_then(|cfg| file_getter(cfg).cloned()))
            .unwrap_or_default()
    }

    pub(super) fn has_configured_value(
        &self,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<usize>,
    ) -> bool {
        raw_getter(&self.env).is_some()
            || raw_getter(&self.git).is_some()
            || self.file.as_ref().and_then(file_getter).is_some()
    }

    pub(super) fn bool_value(
        &self,
        config_key: &str,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<bool>,
        fallback: bool,
    ) -> Result<bool, String> {
        if let Some(raw) = raw_getter(&self.env).or_else(|| raw_getter(&self.git)) {
            return sources::parse_git_bool(raw)
                .ok_or_else(|| format!("invalid {config_key} value {:?}", raw));
        }

        Ok(self.file.as_ref().and_then(file_getter).unwrap_or(fallback))
    }

    pub(super) fn int_value(
        &self,
        config_key: &str,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<usize>,
        fallback: usize,
    ) -> Result<usize, String> {
        if let Some(raw) = raw_getter(&self.env).or_else(|| raw_getter(&self.git)) {
            return sources::parse_positive_usize(raw)
                .ok_or_else(|| format!("invalid {config_key} value {:?}", raw));
        }

        if let Some(value) = self.file.as_ref().and_then(file_getter) {
            if value > 0 {
                return Ok(value);
            }
            return Err(format!("invalid {config_key} value {value:?}"));
        }

        Ok(fallback)
    }

    pub(super) fn optional_int_value(
        &self,
        config_key: &str,
        raw_getter: impl Fn(&RawConfigValues) -> Option<&String>,
        file_getter: impl Fn(&FileConfig) -> Option<usize>,
    ) -> Result<Option<usize>, String> {
        if let Some(raw) = raw_getter(&self.env).or_else(|| raw_getter(&self.git)) {
            return sources::parse_positive_usize(raw)
                .map(Some)
                .ok_or_else(|| format!("invalid {config_key} value {:?}", raw));
        }

        if let Some(value) = self.file.as_ref().and_then(file_getter) {
            if value > 0 {
                return Ok(Some(value));
            }
            return Err(format!("invalid {config_key} value {value:?}"));
        }

        Ok(None)
    }
}

impl Config {
    pub fn should_use_anthropic_transport(&self) -> bool {
        self.provider == Provider::AnthropicCompatible
            || (self.provider == Provider::OpenAiCompatible
                && is_anthropic_compatible_url(&self.api_base))
    }

    pub fn requires_api_key(&self) -> bool {
        if self.should_use_anthropic_transport() {
            return true;
        }

        match self.provider {
            Provider::OpenAiCompatible => true,
            Provider::Ollama => self.is_ollama_cloud(),
            Provider::AnthropicCompatible => true,
        }
    }

    pub fn should_send_bearer_auth(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    pub fn is_local_ollama(&self) -> bool {
        self.provider == Provider::Ollama && is_loopback_url(&self.api_base)
    }

    pub fn is_ollama_cloud(&self) -> bool {
        self.provider == Provider::Ollama && is_ollama_cloud_url(&self.api_base)
    }

    pub fn should_auto_detect_model_context_tokens(&self) -> bool {
        self.provider == Provider::OpenAiCompatible
            && self.model_context_tokens.is_none()
            && is_openrouter_url(&self.api_base)
    }

    pub fn auth_mode_description(&self) -> String {
        if self.should_use_anthropic_transport() {
            return if self.api_key.trim().is_empty() {
                "missing x-api-key".to_string()
            } else {
                "x-api-key".to_string()
            };
        }

        match self.provider {
            Provider::OpenAiCompatible => {
                if self.api_key.trim().is_empty() {
                    "missing bearer token".to_string()
                } else {
                    "bearer token".to_string()
                }
            }
            Provider::AnthropicCompatible => "x-api-key".to_string(),
            Provider::Ollama if self.is_local_ollama() => {
                if self.api_key.trim().is_empty() {
                    "none (local ollama)".to_string()
                } else {
                    "bearer token configured (optional for local ollama)".to_string()
                }
            }
            Provider::Ollama if self.is_ollama_cloud() && self.api_key.trim().is_empty() => {
                "missing bearer token (required for ollama cloud)".to_string()
            }
            Provider::Ollama if self.api_key.trim().is_empty() => "none".to_string(),
            Provider::Ollama => "bearer token".to_string(),
        }
    }

    pub fn diff_budget(&self) -> DiffBudgetConfig {
        match self.max_diff_tokens {
            Some(max_tokens) => DiffBudgetConfig::Tokens {
                max_tokens,
                model_context_tokens: self.model_context_tokens,
            },
            None => DiffBudgetConfig::Bytes {
                max_bytes: self.max_diff_bytes,
            },
        }
    }
}
