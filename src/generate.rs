use crate::git::collect_repo_context;
use crate::openai::{
    GenerationMetrics, StreamOutput, generate_message_with_stream_output,
    resolve_model_context_config,
};
use crate::prompt::load_config_for_interactive_use;
use std::io::IsTerminal;
use std::time::Instant;

pub fn run_generate() -> Result<(), String> {
    let started = Instant::now();
    let cfg = resolve_model_context_config(&load_config_for_interactive_use()?, false);
    let repo_ctx = collect_repo_context(&cfg)?;
    let stream_output = if std::io::stdout().is_terminal() {
        StreamOutput::Stdout
    } else {
        StreamOutput::None
    };
    let (message, metrics) =
        generate_message_with_stream_output(&cfg, &repo_ctx, stream_output, false)?;
    if !matches!(stream_output, StreamOutput::Stdout) {
        println!("{message}");
    }
    log_timing(&cfg, "generate", started, metrics);
    Ok(())
}

pub fn log_timing(
    cfg: &crate::config::Config,
    mode: &str,
    started_at: Instant,
    metrics: GenerationMetrics,
) {
    if !cfg.show_timing {
        return;
    }

    eprintln!(
        "git-ai-commit: {mode} completed in {:?} (api {:?})",
        started_at.elapsed(),
        metrics.api_duration
    );
}
