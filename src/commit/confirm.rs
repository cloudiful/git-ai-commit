use std::env;
use std::io::{self, IsTerminal, Write};

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_PROMPT_LABEL: &str = "\x1b[1;33m";
const ANSI_PROMPT_YES: &str = "\x1b[1;32m";
const ANSI_PROMPT_NO: &str = "\x1b[2m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitConfirmation {
    Proceed,
    Edit,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptColorEnv {
    stderr_is_terminal: bool,
    no_color: bool,
    term: Option<String>,
}

pub(super) fn prompt_for_commit_confirmation() -> Result<CommitConfirmation, String> {
    loop {
        eprint!("{}", commit_confirmation_prompt());
        io::stderr().flush().map_err(|err| err.to_string())?;

        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .map_err(|err| err.to_string())?;

        match parse_commit_confirmation(line.trim()) {
            Some(result) => return Ok(result),
            None => eprintln!(
                "git-ai-commit: enter y to commit, e to edit before commit, or n to cancel."
            ),
        }
    }
}

fn parse_commit_confirmation(input: &str) -> Option<CommitConfirmation> {
    match input.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Some(CommitConfirmation::Proceed),
        "e" | "edit" => Some(CommitConfirmation::Edit),
        "" | "n" | "no" => Some(CommitConfirmation::Cancel),
        _ => None,
    }
}

fn current_prompt_color_env() -> PromptColorEnv {
    PromptColorEnv {
        stderr_is_terminal: io::stderr().is_terminal(),
        no_color: env::var_os("NO_COLOR").is_some(),
        term: env::var("TERM").ok(),
    }
}

fn commit_confirmation_prompt() -> String {
    commit_confirmation_prompt_with(&current_prompt_color_env())
}

fn commit_confirmation_prompt_with(env: &PromptColorEnv) -> String {
    if !commit_prompt_colors_enabled_with(env) {
        return "git-ai-commit: commit now, edit before commit, or cancel? [y=e commit/e=edit/N=cancel] ".to_string();
    }

    format!(
        "git-ai-commit: {ANSI_PROMPT_LABEL}commit now, edit before commit, or cancel?{ANSI_RESET} [{ANSI_PROMPT_YES}y=commit{ANSI_RESET}/e=edit/{ANSI_PROMPT_NO}N=cancel{ANSI_RESET}] "
    )
}

fn commit_prompt_colors_enabled_with(env: &PromptColorEnv) -> bool {
    env.stderr_is_terminal
        && !env.no_color
        && !matches!(env.term.as_deref(), Some(term) if term.eq_ignore_ascii_case("dumb"))
}

#[cfg(test)]
mod tests {
    use super::{
        ANSI_PROMPT_LABEL, ANSI_PROMPT_NO, ANSI_PROMPT_YES, CommitConfirmation, PromptColorEnv,
        commit_confirmation_prompt_with, parse_commit_confirmation,
    };

    #[test]
    fn plain_confirmation_prompt_still_contains_question() {
        let prompt = commit_confirmation_prompt_with(&PromptColorEnv {
            stderr_is_terminal: false,
            no_color: false,
            term: Some("xterm-256color".to_string()),
        });
        assert!(prompt.contains("edit before commit"));
        assert!(prompt.contains("["));
    }

    #[test]
    fn colored_confirmation_prompt_uses_expected_styles_when_enabled() {
        let prompt = commit_confirmation_prompt_with(&PromptColorEnv {
            stderr_is_terminal: true,
            no_color: false,
            term: Some("xterm-256color".to_string()),
        });
        assert!(prompt.contains(ANSI_PROMPT_LABEL));
        assert!(prompt.contains(ANSI_PROMPT_YES));
        assert!(prompt.contains(ANSI_PROMPT_NO));
    }

    #[test]
    fn prompt_colors_disabled_for_no_color_or_dumb_term() {
        for env in [
            PromptColorEnv {
                stderr_is_terminal: true,
                no_color: true,
                term: Some("xterm-256color".to_string()),
            },
            PromptColorEnv {
                stderr_is_terminal: true,
                no_color: false,
                term: Some("dumb".to_string()),
            },
        ] {
            let prompt = commit_confirmation_prompt_with(&env);
            assert!(!prompt.contains(ANSI_PROMPT_LABEL));
        }
    }

    #[test]
    fn parses_confirmation_answers() {
        assert_eq!(
            parse_commit_confirmation("y"),
            Some(CommitConfirmation::Proceed)
        );
        assert_eq!(
            parse_commit_confirmation("edit"),
            Some(CommitConfirmation::Edit)
        );
        assert_eq!(
            parse_commit_confirmation(""),
            Some(CommitConfirmation::Cancel)
        );
        assert_eq!(parse_commit_confirmation("wat"), None);
    }
}
