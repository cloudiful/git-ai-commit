use crate::config::{Config, load_config, load_partial_config};
use std::io::{self, IsTerminal, Write};

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
    eprintln!("git-ai-commit: press Enter on an empty line to cancel setup.");

    let fields = [
        (
            "ai.commit.apiBase",
            "API base",
            "Example: https://api.openai.com/v1",
            existing.api_base.as_str(),
        ),
        (
            "ai.commit.apiKey",
            "API key",
            "Stored in git config --global ai.commit.apiKey",
            existing.api_key.as_str(),
        ),
        (
            "ai.commit.model",
            "Model",
            "Example: gpt-4.1-mini",
            existing.model.as_str(),
        ),
    ];

    for (git_key, label, hint, current) in fields {
        if !current.trim().is_empty() {
            continue;
        }

        let value = prompt_line(label, hint)?;
        git_config_global_set(git_key, &value)?;
    }

    eprintln!("git-ai-commit: saved required AI settings to global git config.");
    Ok(())
}

fn prompt_line(label: &str, hint: &str) -> Result<String, String> {
    if !hint.is_empty() {
        eprintln!("git-ai-commit: {hint}");
    }

    eprint!("git-ai-commit: {label}: ");
    io::stderr().flush().map_err(|err| err.to_string())?;

    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|err| err.to_string())?;
    let value = line.trim().to_string();
    if value.is_empty() {
        return Err("setup canceled".to_string());
    }

    Ok(value)
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
