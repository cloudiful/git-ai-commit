use crate::generate::log_timing;
use crate::git::{collect_repo_context, run_git, run_git_interactive};
use crate::openai::{StreamOutput, generate_message_with_stream_output};
use crate::prompt::{
    git_config_global_set, is_interactive_session, load_config_for_interactive_use,
};
use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::time::Instant;
use tempfile::NamedTempFile;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_PROMPT_LABEL: &str = "\x1b[1;33m";
const ANSI_PROMPT_YES: &str = "\x1b[1;32m";
const ANSI_PROMPT_NO: &str = "\x1b[2m";
const CAI_ALIAS_VALUE: &str = r#"!f() { git ai-commit "$@"; }; f"#;

pub fn run_commit(args: &[String]) -> Result<(), String> {
    if should_bypass_ai_commit(args) {
        return run_plain_commit(args);
    }

    let parsed_args = parse_ai_commit_args(args)?;

    let started = Instant::now();
    let cfg = match load_config_for_interactive_use() {
        Ok(cfg) => cfg,
        Err(err) => return run_plain_commit_with_notice(&parsed_args.forward_args, &err),
    };

    let repo_ctx = match collect_repo_context(&cfg) {
        Ok(ctx) => ctx,
        Err(err) => return run_plain_commit_with_notice(&parsed_args.forward_args, &err),
    };
    if repo_ctx.diff_stat.trim().is_empty() && repo_ctx.diff_patch.trim().is_empty() {
        return run_plain_commit_with_notice(
            &parsed_args.forward_args,
            "no staged changes available for AI prompt",
        );
    }
    if is_interactive_session()
        && parsed_args.show_redactions
        && !repo_ctx.secret_redaction_preview.is_empty()
    {
        eprint!("{}", repo_ctx.secret_redaction_preview);
    }

    eprintln!("git-ai-commit: generating commit message from staged changes...");
    let stream_output = if is_interactive_session() {
        StreamOutput::Stderr
    } else {
        StreamOutput::None
    };
    let (message, metrics) =
        match generate_message_with_stream_output(&cfg, &repo_ctx, stream_output) {
            Ok(value) => value,
            Err(err) => return run_plain_commit_with_notice(&parsed_args.forward_args, &err),
        };

    let mut message_file = write_commit_message_temp_file(&message)?;
    log_timing(&cfg, "commit", started, metrics);

    let mut open_editor = cfg.open_editor;
    if is_interactive_session() && parsed_args.confirm_override.unwrap_or(cfg.confirm_commit) {
        match prompt_for_commit_confirmation()? {
            CommitConfirmation::Proceed => {}
            CommitConfirmation::Edit => open_editor = true,
            CommitConfirmation::Cancel => {
                eprintln!("git-ai-commit: commit canceled.");
                return Ok(());
            }
        }
    }

    let commit_args = build_ai_commit_args(
        message_file.path().to_string_lossy().into_owned(),
        open_editor,
        &parsed_args.forward_args,
    );
    message_file
        .as_file_mut()
        .flush()
        .map_err(|err| err.to_string())?;
    run_git_interactive(None::<&Path>, &commit_args)
}

pub fn run_init_alias(args: &[String]) -> Result<(), String> {
    let mut force = false;
    for arg in args {
        match arg.as_str() {
            "--force" => force = true,
            other => return Err(format!("unknown init-alias flag: {other}")),
        }
    }

    if let Ok(current) = git_config_global_get("alias.cai") {
        let current = current.trim();
        if !current.is_empty() && current != CAI_ALIAS_VALUE && !force {
            eprintln!("git-ai-commit: alias.cai already exists; use --force to replace it");
            return Ok(());
        }
        if current == CAI_ALIAS_VALUE {
            eprintln!("git-ai-commit: alias.cai is already configured");
            return Ok(());
        }
    }

    git_config_global_set("alias.cai", CAI_ALIAS_VALUE)
}

pub fn run_doctor(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err("doctor does not accept arguments".to_string());
    }

    match crate::config::load_config() {
        Ok(cfg) => println!("config: ready (model {})", cfg.model),
        Err(err) => println!("config: not ready ({err})"),
    }

    match run_git(None::<&Path>, ["rev-parse", "--show-toplevel"]) {
        Ok(repo_root) => println!("repo: {}", repo_root.trim()),
        Err(err) => println!("repo: not detected ({err})"),
    }

    Ok(())
}

pub fn should_bypass_ai_commit(args: &[String]) -> bool {
    args.iter().any(|arg| match arg.as_str() {
        "--" | "-m" | "-F" | "-C" | "-c" | "--message" | "--file" | "--reuse-message"
        | "--reedit-message" | "--amend" | "-a" | "--all" | "-i" | "--include" | "-o"
        | "--only" | "--fixup" | "--squash" => true,
        _ if arg.starts_with("-m")
            || arg.starts_with("-F")
            || arg.starts_with("-C")
            || arg.starts_with("-c")
            || arg.starts_with("--message=")
            || arg.starts_with("--file=")
            || arg.starts_with("--reuse-message=")
            || arg.starts_with("--reedit-message=")
            || arg.starts_with("--fixup=")
            || arg.starts_with("--squash=") =>
        {
            true
        }
        _ if arg.starts_with('-') => false,
        _ => true,
    })
}

fn run_plain_commit(args: &[String]) -> Result<(), String> {
    let mut commit_args = vec!["commit".to_string()];
    commit_args.extend(args.iter().cloned());
    run_git_interactive(None::<&Path>, &commit_args)
}

fn run_plain_commit_with_notice(args: &[String], reason: &str) -> Result<(), String> {
    eprintln!("git-ai-commit: falling back to plain git commit: {reason}");
    run_plain_commit(args)
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedAiCommitArgs {
    forward_args: Vec<String>,
    confirm_override: Option<bool>,
    show_redactions: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitConfirmation {
    Proceed,
    Edit,
    Cancel,
}

fn parse_ai_commit_args(args: &[String]) -> Result<ParsedAiCommitArgs, String> {
    let mut forward_args = Vec::with_capacity(args.len());
    let mut confirm_override = None;
    let mut show_redactions = false;

    for arg in args {
        match arg.as_str() {
            "--edit" | "--no-edit" => {
                return Err(format!("unknown git-ai-commit flag: {arg}"));
            }
            "--no-confirm" => confirm_override = Some(false),
            "--show-redactions" => show_redactions = true,
            _ => forward_args.push(arg.clone()),
        }
    }

    Ok(ParsedAiCommitArgs {
        forward_args,
        confirm_override,
        show_redactions,
    })
}

fn build_ai_commit_args(message_file: String, open_editor: bool, args: &[String]) -> Vec<String> {
    let mut commit_args = vec!["commit".to_string()];
    if open_editor {
        commit_args.push("-e".to_string());
    }
    commit_args.push("-F".to_string());
    commit_args.push(message_file);
    commit_args.extend(args.iter().cloned());
    commit_args
}

fn write_commit_message_temp_file(message: &str) -> Result<NamedTempFile, String> {
    let mut file = NamedTempFile::new().map_err(|err| err.to_string())?;
    writeln!(file, "{message}").map_err(|err| err.to_string())?;
    Ok(file)
}

fn prompt_for_commit_confirmation() -> Result<CommitConfirmation, String> {
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
    if !commit_prompt_colors_enabled() {
        return "git-ai-commit: commit now, edit before commit, or cancel? [y=e commit/e=edit/N=cancel] ".to_string();
    }

    format!(
        "git-ai-commit: {ANSI_PROMPT_LABEL}commit now, edit before commit, or cancel?{ANSI_RESET} [{ANSI_PROMPT_YES}y=commit{ANSI_RESET}/e=edit/{ANSI_PROMPT_NO}N=cancel{ANSI_RESET}] "
    )
}

fn commit_prompt_colors_enabled() -> bool {
    if !io::stderr().is_terminal() {
        return false;
    }

    if env::var_os("NO_COLOR").is_some() {
        return false;
    }

    if matches!(env::var("TERM"), Ok(term) if term.eq_ignore_ascii_case("dumb")) {
        return false;
    }

    true
}

fn git_config_global_get(key: &str) -> Result<String, String> {
    run_git(None::<&Path>, ["config", "--global", "--get", key])
}

#[cfg(test)]
mod tests {
    use super::{
        ANSI_PROMPT_LABEL, ANSI_PROMPT_NO, ANSI_PROMPT_YES, CommitConfirmation, ParsedAiCommitArgs,
        build_ai_commit_args, commit_confirmation_prompt, parse_ai_commit_args,
        parse_commit_confirmation, should_bypass_ai_commit,
    };

    #[test]
    fn bypass_rules_match_go_behavior() {
        let cases = vec![
            (vec![], false),
            (vec!["-s"], false),
            (vec!["--no-verify"], false),
            (vec!["--edit"], false),
            (vec!["--no-edit"], false),
            (vec!["-m", "msg"], true),
            (vec!["-mmsg"], true),
            (vec!["--message=msg"], true),
            (vec!["--amend"], true),
            (vec!["--fixup=reword:HEAD"], true),
            (vec!["-a"], true),
            (vec!["README.md"], true),
        ];

        for (args, want) in cases {
            let args = args.into_iter().map(str::to_string).collect::<Vec<_>>();
            assert_eq!(should_bypass_ai_commit(&args), want, "args: {args:?}");
        }
    }

    #[test]
    fn parses_commit_control_flags_without_forwarding_them() {
        let parsed = parse_ai_commit_args(&[
            "--no-confirm".to_string(),
            "--show-redactions".to_string(),
            "-s".to_string(),
        ])
        .expect("expected parsed args");

        assert_eq!(
            parsed,
            ParsedAiCommitArgs {
                forward_args: vec!["-s".to_string()],
                confirm_override: Some(false),
                show_redactions: true,
            }
        );
    }

    #[test]
    fn rejects_legacy_edit_flags() {
        let err = parse_ai_commit_args(&["--edit".to_string()]).expect_err("expected error");
        assert!(err.contains("unknown git-ai-commit flag"));
        assert!(err.contains("--edit"));
    }

    #[test]
    fn builds_direct_commit_args_by_default() {
        let args = build_ai_commit_args("message.txt".to_string(), false, &["-s".to_string()]);
        assert_eq!(
            args,
            vec![
                "commit".to_string(),
                "-F".to_string(),
                "message.txt".to_string(),
                "-s".to_string(),
            ]
        );
    }

    #[test]
    fn builds_editor_commit_args_when_requested() {
        let args = build_ai_commit_args("message.txt".to_string(), true, &["-s".to_string()]);
        assert_eq!(
            args,
            vec![
                "commit".to_string(),
                "-e".to_string(),
                "-F".to_string(),
                "message.txt".to_string(),
                "-s".to_string(),
            ]
        );
    }

    #[test]
    fn plain_confirmation_prompt_still_contains_question() {
        let prompt = commit_confirmation_prompt();
        assert!(prompt.contains("edit before commit"));
        assert!(prompt.contains("["));
    }

    #[test]
    fn colored_confirmation_prompt_uses_expected_styles_when_enabled() {
        if !super::commit_prompt_colors_enabled() {
            return;
        }

        let prompt = commit_confirmation_prompt();
        assert!(prompt.contains(ANSI_PROMPT_LABEL));
        assert!(prompt.contains(ANSI_PROMPT_YES));
        assert!(prompt.contains(ANSI_PROMPT_NO));
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
