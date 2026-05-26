use crate::git::collect_repo_context;
use crate::openai::{
    GenerationMetrics, StreamOutput, generate_message_with_stream_output,
    resolve_model_context_config,
};
use crate::prompt::load_config_for_interactive_use;
use crate::terminal_ui::{stderr_colors_enabled, style_label, style_muted};
use std::io::IsTerminal;
use std::time::Instant;

pub async fn run_generate() -> Result<(), String> {
    let started = Instant::now();
    let cfg = resolve_model_context_config(&load_config_for_interactive_use()?, false).await;
    let repo_ctx = collect_repo_context(&cfg)?;
    let stream_output = if std::io::stdout().is_terminal() {
        StreamOutput::Stdout
    } else {
        StreamOutput::None
    };
    let (message, metrics) =
        generate_message_with_stream_output(&cfg, &repo_ctx, stream_output, false).await?;
    if !metrics.streamed_render_completed {
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

    let colors_enabled = stderr_colors_enabled();
    eprintln!(
        "{}: {}",
        style_label(colors_enabled, "git-ai-commit"),
        style_muted(
            colors_enabled,
            &format!(
                "{mode} completed in {:?} (api {:?})",
                started_at.elapsed(),
                metrics.api_duration
            ),
        ),
    );
}
