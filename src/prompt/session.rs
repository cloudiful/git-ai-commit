use crate::config::Provider;
use std::io::{BufRead, Write};

pub(super) struct PromptSession<'a, R, W> {
    input: &'a mut R,
    output: &'a mut W,
}

impl<'a, R, W> PromptSession<'a, R, W>
where
    R: BufRead,
    W: Write,
{
    pub(super) fn new(input: &'a mut R, output: &'a mut W) -> Self {
        Self { input, output }
    }

    pub(super) fn print_line(&mut self, message: &str) -> Result<(), String> {
        writeln!(self.output, "{message}").map_err(|err| err.to_string())
    }

    pub(super) fn prompt_line(&mut self, label: &str, hint: &str) -> Result<String, String> {
        self.prompt_line_with_optional_default(label, hint, None)?
            .ok_or_else(|| "prompt did not return a value".to_string())
    }

    pub(super) fn prompt_line_with_optional_default(
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
            None => {
                write!(self.output, "git-ai-commit: {label}: ").map_err(|err| err.to_string())?
            }
        }
        self.output.flush().map_err(|err| err.to_string())?;

        let mut line = String::new();
        self.input
            .read_line(&mut line)
            .map_err(|err| err.to_string())?;
        let trimmed = line.trim();
        match (
            trimmed.is_empty(),
            default.filter(|value| !value.trim().is_empty()),
        ) {
            (true, Some(default)) => Ok(Some(default.trim().to_string())),
            (true, None) => Err("setup canceled".to_string()),
            (false, _) => Ok(Some(trimmed.to_string())),
        }
    }

    pub(super) fn prompt_provider(&mut self, current: Provider) -> Result<Provider, String> {
        loop {
            let value = self
                .prompt_line_with_optional_default(
                    "Provider",
                    "Enter openai-compatible or ollama",
                    Some(current.as_config_value()),
                )?
                .ok_or_else(|| "provider prompt did not return a value".to_string())?;
            if let Some(provider) = Provider::parse(&value) {
                return Ok(provider);
            }
            self.print_line(
                "git-ai-commit: provider must be openai-compatible, openai, or ollama.",
            )?;
        }
    }
}
