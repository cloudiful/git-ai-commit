use std::env;
use std::io::{self, IsTerminal};

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_LABEL: &str = "\x1b[1;37m";
const ANSI_SUBJECT: &str = "\x1b[1;96m";
const ANSI_ACCENT: &str = "\x1b[38;5;245m";
const ANSI_MUTED: &str = "\x1b[2m";
const ANSI_SUCCESS: &str = "\x1b[1;32m";
const ANSI_EDIT: &str = "\x1b[1;36m";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalUiEnv {
    pub stderr_is_terminal: bool,
    pub no_color: bool,
    pub term: Option<String>,
}

pub(crate) fn current_stderr_ui_env() -> TerminalUiEnv {
    TerminalUiEnv {
        stderr_is_terminal: io::stderr().is_terminal(),
        no_color: env::var_os("NO_COLOR").is_some(),
        term: env::var("TERM").ok(),
    }
}

pub(crate) fn stderr_colors_enabled() -> bool {
    stderr_colors_enabled_with(&current_stderr_ui_env())
}

pub(crate) fn stderr_colors_enabled_with(env: &TerminalUiEnv) -> bool {
    env.stderr_is_terminal
        && !env.no_color
        && !matches!(env.term.as_deref(), Some(term) if term.eq_ignore_ascii_case("dumb"))
}

pub(crate) fn style_label(colors_enabled: bool, text: &str) -> String {
    style(colors_enabled, ANSI_LABEL, text)
}

pub(crate) fn style_subject(colors_enabled: bool, text: &str) -> String {
    style(colors_enabled, ANSI_SUBJECT, text)
}

pub(crate) fn style_accent(colors_enabled: bool, text: &str) -> String {
    style(colors_enabled, ANSI_ACCENT, text)
}

pub(crate) fn style_muted(colors_enabled: bool, text: &str) -> String {
    style(colors_enabled, ANSI_MUTED, text)
}

pub(crate) fn style_success(colors_enabled: bool, text: &str) -> String {
    style(colors_enabled, ANSI_SUCCESS, text)
}

pub(crate) fn style_edit(colors_enabled: bool, text: &str) -> String {
    style(colors_enabled, ANSI_EDIT, text)
}

fn style(colors_enabled: bool, ansi: &str, text: &str) -> String {
    if colors_enabled {
        format!("{ansi}{text}{ANSI_RESET}")
    } else {
        text.to_string()
    }
}
