mod provider;
mod resolve;
mod sources;
mod types;

#[cfg(test)]
mod tests;

use sources::load_config_snapshot;
use std::time::Duration;

pub use self::provider::{
    DEFAULT_OLLAMA_API_BASE, Provider, is_anthropic_compatible_url, is_loopback_url,
    is_ollama_cloud_url, is_openrouter_url, validate_api_base,
};
pub use self::types::{Config, DiffBudgetConfig, ReasoningEffort};
use self::types::{FileConfig, FileRedactionRules, RawConfigValues};

pub const DEFAULT_TIMEOUT_SEC: u64 = 180;
pub const DEFAULT_MAX_DIFF_TOKENS: usize = 32_000;
pub const MAX_AUTO_DIFF_TOKENS: usize = 64_000;

pub fn default_redaction_rules() -> redactor::RedactionRules {
    use redactor::FindingKind;

    redactor::RedactionRules::default()
        .with_kind(FindingKind::Secret, true)
        .with_kind(FindingKind::Domain, true)
        .with_kind(FindingKind::Url, true)
        .with_kind(FindingKind::Email, true)
        .with_kind(FindingKind::Ip, true)
        .with_kind(FindingKind::Cidr, true)
        .with_kind(FindingKind::Phone, true)
        .with_kind(FindingKind::Organization, true)
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
    if !api_base.is_empty() {
        validate_api_base(&api_base)?;
    }
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
        enable_fallback: snapshot.bool_value(
            "ai.commit.enableFallback",
            |values| values.enable_fallback.as_ref(),
            |cfg| cfg.enable_fallback,
            false,
        )?,
        redact_secrets: snapshot.bool_value(
            "ai.commit.redactSecrets",
            |values| values.redact_secrets.as_ref(),
            |cfg| cfg.redact_secrets,
            true,
        )?,
        redaction_rules: snapshot.redaction_rules()?,
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
        max_diff_tokens: snapshot.int_value(
            "ai.commit.maxDiffTokens",
            |values| values.max_diff_tokens.as_ref(),
            |cfg| cfg.max_diff_tokens,
            DEFAULT_MAX_DIFF_TOKENS,
        )?,
        max_diff_tokens_explicit,
        model_context_tokens: snapshot.optional_int_value(
            "ai.commit.modelContextTokens",
            |values| values.model_context_tokens.as_ref(),
            |cfg| cfg.model_context_tokens,
        )?,
        reasoning_effort: snapshot.reasoning_effort_value(
            "ai.commit.reasoningEffort",
            |values| values.reasoning_effort.as_ref(),
            |cfg| cfg.reasoning_effort.as_deref(),
            ReasoningEffort::Low,
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
