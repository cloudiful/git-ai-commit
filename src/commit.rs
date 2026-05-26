mod args;
mod confirm;
mod doctor;

use crate::generate::log_timing;
use crate::git::{collect_repo_context, run_git_interactive};
use crate::openai::{
    StreamOutput, generate_message_with_stream_output, resolve_model_context_config,
};
use crate::prompt::{is_interactive_session, load_config_for_interactive_use};
use crate::terminal_ui::{
    TerminalUiEnv, current_stderr_ui_env, stderr_colors_enabled_with, style_accent, style_label,
    style_muted, style_subject,
};
use std::io::Write;
use std::path::Path;
use std::time::Instant;
use tempfile::NamedTempFile;

use self::args::{build_ai_commit_args, parse_ai_commit_args, should_bypass_ai_commit};
use self::confirm::{CommitConfirmation, prompt_for_commit_confirmation};

pub async fn run_commit(args: &[String]) -> Result<(), String> {
    if should_bypass_ai_commit(args) {
        return run_plain_commit(args);
    }

    let parsed_args = parse_ai_commit_args(args)?;
    let started = Instant::now();
    let cfg = match load_config_for_interactive_use() {
        Ok(cfg) => cfg,
        Err(err) => return run_plain_commit_with_notice(&parsed_args.forward_args, &err),
    };

    let cfg = resolve_model_context_config(&cfg, parsed_args.debug_provider).await;

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

    let colors_enabled = stderr_colors_enabled_with(&current_stderr_ui_env());
    eprintln!(
        "{}: {}",
        style_label(colors_enabled, "git-ai-commit"),
        style_muted(colors_enabled, "generating commit message from staged changes..."),
    );
    let stream_output = if is_interactive_session() {
        StreamOutput::Stdout
    } else {
        StreamOutput::None
    };
    let (message, metrics) = match generate_message_with_stream_output(
        &cfg,
        &repo_ctx,
        stream_output,
        parsed_args.debug_provider,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return Err(format!(
                "git-ai-commit: failed to generate commit message: {err}"
            ));
        }
    };

    let mut message_file = write_commit_message_temp_file(&message)?;
    log_timing(&cfg, "commit", started, metrics);
    if !metrics.streamed_render_completed {
        eprint!("{}", commit_message_preview(&message));
    }

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

pub async fn run_doctor(args: &[String]) -> Result<(), String> {
    doctor::run_doctor(args).await
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

fn commit_message_preview(message: &str) -> String {
    commit_message_preview_with(&current_stderr_ui_env(), message)
}

fn commit_message_preview_with(env: &TerminalUiEnv, message: &str) -> String {
    let lines = message.lines().collect::<Vec<_>>();
    let subject = lines.first().copied().unwrap_or_default();
    let body = lines.iter().skip(1).copied().collect::<Vec<_>>();
    let colors_enabled = stderr_colors_enabled_with(env);

    let mut out = String::new();
    out.push('\n');
    out.push_str(&format!(
        "{}: {}\n",
        style_label(colors_enabled, "git-ai-commit"),
        if colors_enabled {
            style_muted(colors_enabled, "generated commit message")
        } else {
            "generated commit message".to_string()
        }
    ));
    out.push_str(&format!(
        "  {} {}\n",
        style_accent(colors_enabled, ">"),
        style_subject(colors_enabled, subject)
    ));
    for line in body {
        let content = if line.is_empty() { " " } else { line };
        out.push_str(&format!(
            "  {} {}\n",
            style_accent(colors_enabled, "|"),
            style_muted(colors_enabled, content)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::commit_message_preview_with;
    use crate::terminal_ui::{TerminalUiEnv, style_accent, style_label, style_subject};

    #[test]
    fn plain_preview_formats_subject_and_body() {
        let preview = commit_message_preview_with(
            &TerminalUiEnv {
                stderr_is_terminal: false,
                no_color: false,
                term: Some("xterm-256color".to_string()),
            },
            "feat: add preview\n\nExplain body",
        );

        assert!(preview.contains("generated commit message"));
        assert!(preview.contains("  > feat: add preview"));
        assert!(preview.contains("  |  "));
        assert!(preview.contains("  | Explain body"));
    }

    #[test]
    fn colored_preview_uses_expected_styles() {
        let env = TerminalUiEnv {
            stderr_is_terminal: true,
            no_color: false,
            term: Some("xterm-256color".to_string()),
        };
        let preview = commit_message_preview_with(
            &env,
            "fix: tighten prompt",
        );

        assert!(preview.contains(&style_label(true, "git-ai-commit")));
        assert!(preview.contains(&style_accent(true, ">")));
        assert!(preview.contains(&style_subject(true, "fix: tighten prompt")));
    }
}
