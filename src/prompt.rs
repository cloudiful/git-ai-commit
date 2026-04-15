use crate::config::{Config, DEFAULT_OLLAMA_API_BASE, Provider, is_ollama_cloud_url, load_config, load_partial_config};
use std::io::{self, BufRead, IsTerminal, Write};

pub fn load_config_for_interactive_use() -> Result<Config, String> {
    match load_config() {
        Ok(cfg) => Ok(cfg),
        Err(err) if err.starts_with("missing ") && is_interactive_session() => {
            eprintln!("git-ai-commit: AI settings are not configured yet.");
            let partial = load_partial_config()?;
            prompt_for_missing_config(&partial)?;
            load_config()
        }
        Err(err) => Err(err),
    }
}

fn prompt_for_missing_config(existing: &Config) -> Result<(), String> {
    let stdin = io::stdin();
    let stderr = io::stderr();
    let mut input = stdin.lock();
    let mut output = stderr.lock();
    prompt_for_missing_config_with(existing, &mut input, &mut output, git_config_global_set)
}

fn prompt_for_missing_config_with<R, W, F>(
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
    session.print_line("git-ai-commit: saved required AI settings to global git config.")?;
    Ok(())
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
        }
    }

    fn requires_api_key(self, provider: Provider, api_base: &str) -> bool {
        match provider {
            Provider::OpenAiCompatible => true,
            Provider::Ollama => is_ollama_cloud_url(api_base),
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
    let should_reprompt_provider_fields = provider != existing.provider;
    let provider_profile = ProviderPromptProfile::for_provider(provider);

    let api_base = prompt_api_base(
        existing,
        should_reprompt_provider_fields,
        provider_profile,
        session,
    )?;
    let effective_api_base = api_base
        .as_deref()
        .unwrap_or(existing.api_base.as_str())
        .to_string();
    let api_key = prompt_api_key(
        existing,
        should_reprompt_provider_fields,
        &effective_api_base,
        provider,
        provider_profile,
        session,
    )?;
    let model = prompt_model(
        existing,
        should_reprompt_provider_fields,
        provider_profile,
        session,
    )?;

    Ok(PendingInteractiveConfig {
        provider,
        api_base,
        api_key,
        model,
    })
}

fn prompt_api_base<R, W>(
    existing: &Config,
    should_reprompt_provider_fields: bool,
    provider_profile: ProviderPromptProfile,
    session: &mut PromptSession<'_, R, W>,
) -> Result<Option<String>, String>
where
    R: BufRead,
    W: Write,
{
    if !should_reprompt_provider_fields && !existing.api_base.trim().is_empty() {
        return Ok(None);
    }

    let default = if should_reprompt_provider_fields {
        provider_profile.default_api_base
    } else {
        Some(existing.api_base.as_str())
    };

    session.prompt_line_with_optional_default("API base", provider_profile.api_base_hint, default)
}

fn prompt_api_key<R, W>(
    existing: &Config,
    should_reprompt_provider_fields: bool,
    api_base: &str,
    provider: Provider,
    provider_profile: ProviderPromptProfile,
    session: &mut PromptSession<'_, R, W>,
) -> Result<Option<String>, String>
where
    R: BufRead,
    W: Write,
{
    if !provider_profile.requires_api_key(provider, api_base)
        || (!should_reprompt_provider_fields && !existing.api_key.trim().is_empty())
    {
        return Ok(None);
    }

    session
        .prompt_line("API key", "Stored in git config --global ai.commit.apiKey")
        .map(Some)
}

fn prompt_model<R, W>(
    existing: &Config,
    should_reprompt_provider_fields: bool,
    provider_profile: ProviderPromptProfile,
    session: &mut PromptSession<'_, R, W>,
) -> Result<Option<String>, String>
where
    R: BufRead,
    W: Write,
{
    if !should_reprompt_provider_fields && !existing.model.trim().is_empty() {
        return Ok(None);
    }

    session
        .prompt_line("Model", provider_profile.model_hint)
        .map(Some)
}

fn write_interactive_config_with<F>(
    pending: &PendingInteractiveConfig,
    write_config: &mut F,
) -> Result<(), String>
where
    F: FnMut(&str, &str) -> Result<(), String>,
{
    write_config("ai.commit.provider", pending.provider.as_config_value())?;
    if let Some(value) = pending.api_base.as_deref() {
        write_config("ai.commit.apiBase", value)?;
    }
    if let Some(value) = pending.api_key.as_deref() {
        write_config("ai.commit.apiKey", value)?;
    }
    if let Some(value) = pending.model.as_deref() {
        write_config("ai.commit.model", value)?;
    }
    Ok(())
}

struct PromptSession<'a, R, W> {
    input: &'a mut R,
    output: &'a mut W,
}

impl<'a, R, W> PromptSession<'a, R, W>
where
    R: BufRead,
    W: Write,
{
    fn new(input: &'a mut R, output: &'a mut W) -> Self {
        Self { input, output }
    }

    fn print_line(&mut self, message: &str) -> Result<(), String> {
        writeln!(self.output, "{message}").map_err(|err| err.to_string())
    }

    fn prompt_line(&mut self, label: &str, hint: &str) -> Result<String, String> {
        self.prompt_line_with_optional_default(label, hint, None)
            .map(|value| value.expect("prompt_line always returns a value"))
    }

    fn prompt_line_with_optional_default(
        &mut self,
        label: &str,
        hint: &str,
        default: Option<&str>,
    ) -> Result<Option<String>, String> {
        if !hint.is_empty() {
            self.print_line(&format!("git-ai-commit: {hint}"))?;
        }

        match default.filter(|value| !value.trim().is_empty()) {
            Some(default) => write!(self.output, "git-ai-commit: {label} [{default}]: ")
                .map_err(|err| err.to_string())?,
            None => write!(self.output, "git-ai-commit: {label}: ")
                .map_err(|err| err.to_string())?,
        }
        self.output.flush().map_err(|err| err.to_string())?;

        let mut line = String::new();
        self.input
            .read_line(&mut line)
            .map_err(|err| err.to_string())?;
        let trimmed = line.trim();
        match (trimmed.is_empty(), default.filter(|value| !value.trim().is_empty())) {
            (true, Some(default)) => Ok(Some(default.trim().to_string())),
            (true, None) => Err("setup canceled".to_string()),
            (false, _) => Ok(Some(trimmed.to_string())),
        }
    }

    fn prompt_provider(&mut self, current: Provider) -> Result<Provider, String> {
        loop {
            let value = self
                .prompt_line_with_optional_default(
                    "Provider",
                    "Enter openai-compatible or ollama",
                    Some(current.as_config_value()),
                )?
                .expect("provider prompt always returns a value");
            if let Some(provider) = Provider::parse(&value) {
                return Ok(provider);
            }
            self.print_line(
                "git-ai-commit: provider must be openai-compatible, openai, or ollama.",
            )?;
        }
    }
}

pub fn is_interactive_session() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal()
}

pub fn git_config_global_set(key: &str, value: &str) -> Result<(), String> {
    let status = std::process::Command::new("git")
        .args(["config", "--global", key, value])
        .status()
        .map_err(|err| err.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "git config --global {key} failed with status {status}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::prompt_for_missing_config_with;
    use crate::config::{Config, Provider, DEFAULT_OLLAMA_API_BASE};
    use std::io::Cursor;
    use std::time::Duration;

    #[test]
    fn canceling_at_api_base_does_not_write_anything() {
        let existing = sample_config(Provider::Ollama, DEFAULT_OLLAMA_API_BASE, "", "");
        let mut input = Cursor::new(b"openai-compatible\n\n".as_slice());
        let mut output = Vec::new();
        let mut writes = Vec::new();

        let err = prompt_for_missing_config_with(&existing, &mut input, &mut output, |key, value| {
            writes.push((key.to_string(), value.to_string()));
            Ok(())
        })
        .expect_err("expected setup cancellation");

        assert_eq!(err, "setup canceled");
        assert!(writes.is_empty());
    }

    #[test]
    fn canceling_at_api_key_does_not_write_anything() {
        let existing = sample_config(Provider::OpenAiCompatible, "", "", "");
        let mut input = Cursor::new(b"\nhttps://example.com/v1\n\n".as_slice());
        let mut output = Vec::new();
        let mut writes = Vec::new();

        let err = prompt_for_missing_config_with(&existing, &mut input, &mut output, |key, value| {
            writes.push((key.to_string(), value.to_string()));
            Ok(())
        })
        .expect_err("expected setup cancellation");

        assert_eq!(err, "setup canceled");
        assert!(writes.is_empty());
    }

    #[test]
    fn writes_all_required_openai_fields_after_collecting_everything() {
        let cases = vec![
            (
                sample_config(Provider::OpenAiCompatible, "", "", ""),
                b"\nhttps://example.com/v1\nsecret-token\ngpt-4.1-mini\n".as_slice(),
                vec![
                    (
                        "ai.commit.provider".to_string(),
                        "openai-compatible".to_string(),
                    ),
                    (
                        "ai.commit.apiBase".to_string(),
                        "https://example.com/v1".to_string(),
                    ),
                    ("ai.commit.apiKey".to_string(), "secret-token".to_string()),
                    ("ai.commit.model".to_string(), "gpt-4.1-mini".to_string()),
                ],
            ),
            (
                sample_config(
                    Provider::OpenAiCompatible,
                    "https://api.openai.com/v1",
                    "secret-token",
                    "gpt-4.1-mini",
                ),
                b"ollama\n\nllama3.2\n".as_slice(),
                vec![
                    ("ai.commit.provider".to_string(), "ollama".to_string()),
                    (
                        "ai.commit.apiBase".to_string(),
                        DEFAULT_OLLAMA_API_BASE.to_string(),
                    ),
                    ("ai.commit.model".to_string(), "llama3.2".to_string()),
                ],
            ),
            (
                sample_config(
                    Provider::OpenAiCompatible,
                    "https://api.openai.com/v1",
                    "secret-token",
                    "gpt-4.1-mini",
                ),
                b"ollama\nhttp://10.0.0.5:11434\nqwen3:8b\n".as_slice(),
                vec![
                    ("ai.commit.provider".to_string(), "ollama".to_string()),
                    (
                        "ai.commit.apiBase".to_string(),
                        "http://10.0.0.5:11434".to_string(),
                    ),
                    ("ai.commit.model".to_string(), "qwen3:8b".to_string()),
                ],
            ),
        ];

        for (existing, input_bytes, expected_writes) in cases {
            let mut input = Cursor::new(input_bytes);
            let mut output = Vec::new();
            let mut writes = Vec::new();

            prompt_for_missing_config_with(&existing, &mut input, &mut output, |key, value| {
                writes.push((key.to_string(), value.to_string()));
                Ok(())
            })
            .expect("expected setup to succeed");

            assert_eq!(writes, expected_writes);
        }
    }

    fn sample_config(provider: Provider, api_base: &str, api_key: &str, model: &str) -> Config {
        Config {
            provider,
            api_base: api_base.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            confirm_commit: true,
            open_editor: false,
            redact_secrets: true,
            show_timing: true,
            use_env_proxy: false,
            timeout: Duration::from_secs(5),
            max_diff_bytes: 60_000,
            max_diff_tokens: Some(16_000),
            model_context_tokens: None,
        }
    }
}
