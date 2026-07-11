use super::git_config_global_set;
use super::session::PromptSession;
use crate::config::{Config, DEFAULT_OLLAMA_API_BASE, Provider};
use std::io::{self, BufRead, Write};

pub(super) fn prompt_for_missing_config(existing: &Config) -> Result<(), String> {
    let stdin = io::stdin();
    let stderr = io::stderr();
    prompt_for_missing_config_with(
        existing,
        &mut stdin.lock(),
        &mut stderr.lock(),
        git_config_global_set,
    )
}

pub(super) fn prompt_for_missing_config_with<R, W, F>(
    existing: &Config,
    input: &mut R,
    output: &mut W,
    mut write_config: F,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
    F: FnMut(&str, &str) -> Result<(), String>,
{
    let mut session = PromptSession::new(input, output);
    session.print_line("git-ai-commit: press Enter on an empty line to cancel setup.")?;
    let pending = collect_pending_interactive_config(existing, &mut session)?;
    write_interactive_config_with(&pending, &mut write_config)?;
    session.print_line("git-ai-commit: saved required AI settings to global git config.")
}

#[derive(Debug, PartialEq, Eq)]
struct PendingInteractiveConfig {
    provider: Provider,
    api_base: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
}

#[derive(Clone, Copy)]
struct ProviderPromptProfile {
    api_base_hint: &'static str,
    default_api_base: Option<&'static str>,
    model_hint: &'static str,
}

impl ProviderPromptProfile {
    fn for_provider(provider: Provider) -> Self {
        match provider {
            Provider::OpenAiCompatible => Self {
                api_base_hint: "Example: https://api.openai.com/v1",
                default_api_base: None,
                model_hint: "Example: gpt-4.1-mini",
            },
            Provider::Ollama => Self {
                api_base_hint: "Default local Ollama endpoint: http://localhost:11434",
                default_api_base: Some(DEFAULT_OLLAMA_API_BASE),
                model_hint: "Example: llama3.2 or qwen3:8b",
            },
            Provider::AnthropicCompatible => Self {
                api_base_hint: "Example: https://api.deepseek.com/anthropic",
                default_api_base: None,
                model_hint: "Example: deepseek-chat",
            },
        }
    }
}

fn collect_pending_interactive_config<R, W>(
    existing: &Config,
    session: &mut PromptSession<'_, R, W>,
) -> Result<PendingInteractiveConfig, String>
where
    R: BufRead,
    W: Write,
{
    let provider = session.prompt_provider(existing.provider)?;
    let provider_changed = provider != existing.provider;
    let profile = ProviderPromptProfile::for_provider(provider);
    let api_base = prompt_api_base(existing, provider_changed, profile, session)?;
    let effective_api_base = api_base.as_deref().unwrap_or(&existing.api_base);
    let api_key = prompt_api_key(
        existing,
        provider_changed,
        effective_api_base,
        provider,
        session,
    )?;
    let model = prompt_model(existing, provider_changed, profile, session)?;
    Ok(PendingInteractiveConfig {
        provider,
        api_base,
        api_key,
        model,
    })
}

fn prompt_api_base<R: BufRead, W: Write>(
    existing: &Config,
    provider_changed: bool,
    profile: ProviderPromptProfile,
    session: &mut PromptSession<'_, R, W>,
) -> Result<Option<String>, String> {
    if !provider_changed && !existing.api_base.trim().is_empty() {
        return Ok(None);
    }
    let default = if provider_changed {
        profile.default_api_base
    } else {
        Some(existing.api_base.as_str())
    };
    session.prompt_line_with_optional_default("API base", profile.api_base_hint, default)
}

fn prompt_api_key<R: BufRead, W: Write>(
    existing: &Config,
    provider_changed: bool,
    api_base: &str,
    provider: Provider,
    session: &mut PromptSession<'_, R, W>,
) -> Result<Option<String>, String> {
    if !Config::provider_requires_api_key(provider, api_base)
        || (!provider_changed && !existing.api_key.trim().is_empty())
    {
        return Ok(None);
    }
    session
        .prompt_line("API key", "Stored in git config --global ai.commit.apiKey")
        .map(Some)
}

fn prompt_model<R: BufRead, W: Write>(
    existing: &Config,
    provider_changed: bool,
    profile: ProviderPromptProfile,
    session: &mut PromptSession<'_, R, W>,
) -> Result<Option<String>, String> {
    if !provider_changed && !existing.model.trim().is_empty() {
        return Ok(None);
    }
    session.prompt_line("Model", profile.model_hint).map(Some)
}

fn write_interactive_config_with<F>(
    pending: &PendingInteractiveConfig,
    write_config: &mut F,
) -> Result<(), String>
where
    F: FnMut(&str, &str) -> Result<(), String>,
{
    write_config("ai.commit.provider", pending.provider.as_config_value())?;
    for (key, value) in [
        ("ai.commit.apiBase", pending.api_base.as_deref()),
        ("ai.commit.apiKey", pending.api_key.as_deref()),
        ("ai.commit.model", pending.model.as_deref()),
    ] {
        if let Some(value) = value {
            write_config(key, value)?;
        }
    }
    Ok(())
}
