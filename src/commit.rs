mod alias;
mod args;
mod confirm;
mod doctor;

use crate::generate::log_timing;
use crate::git::{collect_repo_context, run_git_interactive};
use crate::openai::{
    StreamOutput, generate_message_with_stream_output, resolve_model_context_config,
};
use crate::prompt::{is_interactive_session, load_config_for_interactive_use};
use std::io::Write;
use std::path::Path;
use std::time::Instant;
use tempfile::NamedTempFile;

use self::args::{build_ai_commit_args, parse_ai_commit_args, should_bypass_ai_commit};
use self::confirm::{CommitConfirmation, prompt_for_commit_confirmation};

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

    let cfg = resolve_model_context_config(&cfg, parsed_args.debug_provider);

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
    let (message, metrics) = match generate_message_with_stream_output(
        &cfg,
        &repo_ctx,
        stream_output,
        parsed_args.debug_provider,
    ) {
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
    alias::run_init_alias(args)
}

pub fn run_doctor(args: &[String]) -> Result<(), String> {
    doctor::run_doctor(args)
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

fn write_commit_message_temp_file(message: &str) -> Result<NamedTempFile, String> {
    let mut file = NamedTempFile::new().map_err(|err| err.to_string())?;
    writeln!(file, "{message}").map_err(|err| err.to_string())?;
    Ok(file)
}
