use crate::git::collect_repo_context;
use crate::openai::{
    GenerationMetrics, StreamOutput, generate_message_with_stream_output,
    resolve_model_context_config,
};
use crate::prompt::load_config_for_interactive_use;
use crate::terminal_ui::{stderr_colors_enabled, style_label, style_muted, style_success};
use std::io::IsTerminal;
use std::time::{Duration, Instant};

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
    let total = format_compact_duration(started_at.elapsed());
    let api = format_compact_duration(metrics.api_duration);
    eprintln!(
        "{}: {} {}",
        style_label(colors_enabled, "git-ai-commit"),
        style_success(colors_enabled, &format!("{mode} ready")),
        style_muted(colors_enabled, &format!("in {total} (ai {api})"),),
    );
}

fn format_compact_duration(duration: Duration) -> String {
    let secs = duration.as_secs_f64();

    if secs >= 60.0 {
        let minutes = duration.as_secs() / 60;
        let remaining = secs - (minutes as f64 * 60.0);
        return format!("{minutes}m {:.2}s", remaining);
    }

    if secs >= 1.0 {
        return format!("{secs:.2}s");
    }

    let millis = duration.as_millis();
    format!("{millis}ms")
}

#[cfg(test)]
mod tests {
    use super::format_compact_duration;
    use std::time::Duration;

    #[test]
    fn formats_subsecond_durations_as_millis() {
        assert_eq!(format_compact_duration(Duration::from_millis(317)), "317ms");
    }

    #[test]
    fn formats_seconds_with_two_decimals() {
        assert_eq!(
            format_compact_duration(Duration::from_millis(8513)),
            "8.51s"
        );
    }

    #[test]
    fn formats_minute_scale_durations_compactly() {
        assert_eq!(
            format_compact_duration(Duration::from_millis(72340)),
            "1m 12.34s"
        );
    }
}
