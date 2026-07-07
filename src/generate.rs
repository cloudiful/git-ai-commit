use crate::generation::{StreamOutputMode, execute_generation, prepare_generation};
use crate::terminal_ui::{stderr_colors_enabled, style_label, style_muted};
use std::time::{Duration, Instant};

pub async fn run_generate() -> Result<(), String> {
    let started = Instant::now();
    let prepared = prepare_generation(false, StreamOutputMode::StdoutTerminal).await?;
    let generated = execute_generation(&prepared, false).await?;
    if !generated.streamed_render_completed {
        println!("{}", generated.message);
    }
    log_timing(&prepared.cfg, started);
    Ok(())
}

pub fn log_timing(cfg: &crate::config::Config, started_at: Instant) {
    let Some(summary) = timing_summary(cfg, started_at) else {
        return;
    };

    let colors_enabled = stderr_colors_enabled();
    eprintln!(
        "{}: {}",
        style_label(colors_enabled, "git-ai-commit"),
        style_muted(colors_enabled, &summary),
    );
}

pub fn timing_summary(cfg: &crate::config::Config, started_at: Instant) -> Option<String> {
    if !cfg.show_timing {
        return None;
    }

    let total = format_compact_duration(started_at.elapsed());
    Some(format!("ready {total}"))
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
