mod session;
mod setup;

#[cfg(test)]
mod tests;

use crate::config::{Config, load_config, load_partial_config};
use std::io::{self, IsTerminal};

pub fn load_config_for_interactive_use() -> Result<Config, String> {
    match load_config() {
        Ok(cfg) => Ok(cfg),
        Err(err) if err.starts_with("missing ") && is_interactive_session() => {
            eprintln!("git-ai-commit: AI settings are not configured yet.");
            let partial = load_partial_config()?;
            setup::prompt_for_missing_config(&partial)?;
            load_config()
        }
        Err(err) => Err(err),
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
