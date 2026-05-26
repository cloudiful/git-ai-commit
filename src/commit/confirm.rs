use crate::terminal_ui::{
    TerminalUiEnv, current_stderr_ui_env, stderr_colors_enabled_with, style_edit, style_label,
    style_muted, style_success,
};
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitConfirmation {
    Proceed,
    Edit,
    Cancel,
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

fn commit_confirmation_prompt() -> String {
    commit_confirmation_prompt_with(&current_stderr_ui_env())
}

fn commit_confirmation_prompt_with(env: &TerminalUiEnv) -> String {
    let colors_enabled = stderr_colors_enabled_with(env);
    if !colors_enabled {
        return "git-ai-commit: continue? [y=commit/e=edit/N=cancel] ".to_string();
    }

    format!(
        "{}: {} [{}/{}/{}] ",
        style_label(colors_enabled, "git-ai-commit"),
        style_muted(colors_enabled, "continue?"),
        style_success(colors_enabled, "y=commit"),
        style_edit(colors_enabled, "e=edit"),
        style_muted(colors_enabled, "N=cancel"),
    )
}

#[cfg(test)]
mod tests {
    use super::{CommitConfirmation, commit_confirmation_prompt_with, parse_commit_confirmation};
    use crate::terminal_ui::{
        TerminalUiEnv, style_edit, style_label, style_muted, style_success,
    };

    #[test]
    fn plain_confirmation_prompt_still_contains_question() {
        let prompt = commit_confirmation_prompt_with(&TerminalUiEnv {
            stderr_is_terminal: false,
            no_color: false,
            term: Some("xterm-256color".to_string()),
        });
        assert!(prompt.contains("continue?"));
        assert!(prompt.contains("y=commit"));
        assert!(prompt.contains("["));
    }

    #[test]
    fn colored_confirmation_prompt_uses_expected_styles_when_enabled() {
        let env = TerminalUiEnv {
            stderr_is_terminal: true,
            no_color: false,
            term: Some("xterm-256color".to_string()),
        };
        let prompt = commit_confirmation_prompt_with(&env);
        assert!(prompt.contains(&style_label(true, "git-ai-commit")));
        assert!(prompt.contains(&style_success(true, "y=commit")));
        assert!(prompt.contains(&style_edit(true, "e=edit")));
        assert!(prompt.contains(&style_muted(true, "N=cancel")));
    }

    #[test]
    fn prompt_colors_disabled_for_no_color_or_dumb_term() {
        for env in [
            TerminalUiEnv {
                stderr_is_terminal: true,
                no_color: true,
                term: Some("xterm-256color".to_string()),
            },
            TerminalUiEnv {
                stderr_is_terminal: true,
                no_color: false,
                term: Some("dumb".to_string()),
            },
        ] {
            let prompt = commit_confirmation_prompt_with(&env);
            assert!(!prompt.contains("\x1b["));
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
