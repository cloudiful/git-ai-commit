use super::{
    Provider, is_anthropic_compatible_url, is_loopback_url, is_ollama_cloud_url, is_openrouter_url,
};
use redactor::RedactionRules;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReasoningEffort {
    #[default]
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    pub fn as_api_value(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffBudgetConfig {
    pub max_tokens: usize,
    pub model_context_tokens: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub provider: Provider,
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub confirm_commit: bool,
    pub open_editor: bool,
    pub enable_fallback: bool,
    pub redact_secrets: bool,
    pub redaction_rules: RedactionRules,
    pub show_timing: bool,
    pub use_env_proxy: bool,
    pub timeout: Duration,
    pub max_diff_tokens: usize,
    pub max_diff_tokens_explicit: bool,
    pub model_context_tokens: Option<usize>,
    pub reasoning_effort: ReasoningEffort,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct FileConfig {
    pub(super) provider: Option<String>,
    pub(super) api_base: Option<String>,
    pub(super) api_key: Option<String>,
    pub(super) model: Option<String>,
    pub(super) confirm_commit: Option<bool>,
    pub(super) open_editor: Option<bool>,
    pub(super) enable_fallback: Option<bool>,
    pub(super) redact_secrets: Option<bool>,
    pub(super) redaction_rules: Option<FileRedactionRules>,
    pub(super) show_timing: Option<bool>,
    pub(super) use_env_proxy: Option<bool>,
    pub(super) timeout_sec: Option<usize>,
    pub(super) max_diff_tokens: Option<usize>,
    pub(super) model_context_tokens: Option<usize>,
    pub(super) reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct FileRedactionRules {
    pub(super) secret: Option<bool>,
    pub(super) domain: Option<bool>,
    pub(super) url: Option<bool>,
    pub(super) email: Option<bool>,
    pub(super) ip: Option<bool>,
    pub(super) cidr: Option<bool>,
    pub(super) phone: Option<bool>,
    pub(super) person: Option<bool>,
    pub(super) organization: Option<bool>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RawConfigValues {
    pub(super) provider: Option<String>,
    pub(super) api_base: Option<String>,
    pub(super) api_key: Option<String>,
    pub(super) model: Option<String>,
    pub(super) confirm_commit: Option<String>,
    pub(super) open_editor: Option<String>,
    pub(super) enable_fallback: Option<String>,
    pub(super) redact_secrets: Option<String>,
    pub(super) redaction_secret: Option<String>,
    pub(super) redaction_domain: Option<String>,
    pub(super) redaction_url: Option<String>,
    pub(super) redaction_email: Option<String>,
    pub(super) redaction_ip: Option<String>,
    pub(super) redaction_cidr: Option<String>,
    pub(super) redaction_phone: Option<String>,
    pub(super) redaction_person: Option<String>,
    pub(super) redaction_organization: Option<String>,
    pub(super) show_timing: Option<String>,
    pub(super) use_env_proxy: Option<String>,
    pub(super) timeout_sec: Option<String>,
    pub(super) max_diff_tokens: Option<String>,
    pub(super) model_context_tokens: Option<String>,
    pub(super) reasoning_effort: Option<String>,
}

impl Config {
    pub fn provider_requires_api_key(provider: Provider, api_base: &str) -> bool {
        match provider {
            Provider::OpenAiCompatible => true,
            Provider::Ollama => is_ollama_cloud_url(api_base),
            Provider::AnthropicCompatible => true,
        }
    }

    pub fn should_use_anthropic_transport(&self) -> bool {
        self.provider == Provider::AnthropicCompatible
            || (self.provider == Provider::OpenAiCompatible
                && is_anthropic_compatible_url(&self.api_base))
    }

    pub fn requires_api_key(&self) -> bool {
        self.should_use_anthropic_transport()
            || Self::provider_requires_api_key(self.provider, &self.api_base)
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

    pub fn should_use_streaming_generation(&self) -> bool {
        !self.should_use_anthropic_transport()
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
            Provider::OpenAiCompatible if self.api_key.trim().is_empty() => {
                "missing bearer token".to_string()
            }
            Provider::OpenAiCompatible => "bearer token".to_string(),
            Provider::AnthropicCompatible => "x-api-key".to_string(),
            Provider::Ollama if self.is_local_ollama() && self.api_key.trim().is_empty() => {
                "none (local ollama)".to_string()
            }
            Provider::Ollama if self.is_local_ollama() => {
                "bearer token configured (optional for local ollama)".to_string()
            }
            Provider::Ollama if self.is_ollama_cloud() && self.api_key.trim().is_empty() => {
                "missing bearer token (required for ollama cloud)".to_string()
            }
            Provider::Ollama if self.api_key.trim().is_empty() => "none".to_string(),
            Provider::Ollama => "bearer token".to_string(),
        }
    }

    pub fn diff_budget(&self) -> DiffBudgetConfig {
        DiffBudgetConfig {
            max_tokens: self.max_diff_tokens,
            model_context_tokens: self.model_context_tokens,
        }
    }
}
