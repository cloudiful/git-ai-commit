use super::super::stream_palette::StreamPalette;
use std::io::Write;
use std::sync::{Arc, Mutex};

const ANSI_CLEAR_LINE: &str = "\x1b[2K";
const ANSI_THINKING: &str = "\x1b[90m";
const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const LABEL: &str = "thinking";
const STATUS_WINDOW_WIDTH: usize = 56;
const STATUS_SCROLL_GAP: usize = 8;
const GRADIENT_START: (u8, u8, u8) = (142, 148, 163);
const GRADIENT_END: (u8, u8, u8) = (110, 198, 214);
const WAVE_HIGHLIGHT: (u8, u8, u8) = (230, 246, 255);

pub(super) fn render_status(
    output_lock: &Arc<Mutex<()>>,
    palette: StreamPalette,
    frame_index: usize,
    text: &str,
) -> std::io::Result<()> {
    let _guard = output_lock
        .lock()
        .map_err(|_| std::io::Error::other("stdout lock poisoned"))?;
    let mut stdout = std::io::stdout().lock();
    write_status_line(&mut stdout, palette, frame_index, text)?;
    stdout.flush()
}

pub(super) fn clear_status_line(output_lock: &Arc<Mutex<()>>) -> std::io::Result<()> {
    let _guard = output_lock
        .lock()
        .map_err(|_| std::io::Error::other("stdout lock poisoned"))?;
    let mut stdout = std::io::stdout().lock();
    write!(stdout, "\r{ANSI_CLEAR_LINE}\r")?;
    stdout.flush()
}

pub(super) fn write_status_line<W: Write>(
    writer: &mut W,
    palette: StreamPalette,
    frame_index: usize,
    text: &str,
) -> std::io::Result<()> {
    let frame = SPINNER_FRAMES[frame_index % SPINNER_FRAMES.len()];
    let window = scrolling_window_text(text, frame_index);
    let line = if window.is_empty() {
        LABEL.to_string()
    } else {
        format!("{LABEL}: {window}")
    };
    write!(writer, "\r{ANSI_CLEAR_LINE}\r")?;
    if palette.supports_truecolor() {
        write_wave_gradient(writer, frame, &line, frame_index)?;
        palette.write_reset(writer)?;
    } else if palette.colors_enabled() {
        write!(writer, "{ANSI_THINKING}{frame} {line}")?;
        palette.write_reset(writer)?;
    } else {
        write!(writer, "{frame} {line}")?;
    }
    Ok(())
}

pub(super) fn scrolling_window_text(text: &str, frame_index: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return String::new();
    }
    let chars = compact.chars().collect::<Vec<_>>();
    if chars.len() <= STATUS_WINDOW_WIDTH {
        return pad_right(compact, STATUS_WINDOW_WIDTH);
    }
    let mut marquee = chars.clone();
    marquee.extend(std::iter::repeat_n(' ', STATUS_SCROLL_GAP));
    let start = (frame_index / 2) % marquee.len();
    marquee
        .iter()
        .cycle()
        .skip(start)
        .take(STATUS_WINDOW_WIDTH)
        .collect()
}

fn pad_right(mut text: String, width: usize) -> String {
    let len = text.chars().count();
    text.extend(std::iter::repeat_n(' ', width.saturating_sub(len)));
    text
}

fn write_wave_gradient<W: Write>(
    writer: &mut W,
    frame: &str,
    text: &str,
    frame_index: usize,
) -> std::io::Result<()> {
    let chars = text.chars().collect::<Vec<_>>();
    write_truecolor_text(writer, frame, GRADIENT_END)?;
    write!(writer, " ")?;
    for (idx, ch) in chars.iter().enumerate() {
        let color = wave_color(
            interpolate_color(idx, chars.len()),
            idx,
            chars.len(),
            frame_index,
        );
        write_truecolor_text(writer, &ch.to_string(), color)?;
    }
    Ok(())
}

fn interpolate_color(idx: usize, len: usize) -> (u8, u8, u8) {
    if len <= 1 {
        return GRADIENT_END;
    }
    let t = idx as f32 / (len - 1) as f32;
    (
        lerp(GRADIENT_START.0, GRADIENT_END.0, t),
        lerp(GRADIENT_START.1, GRADIENT_END.1, t),
        lerp(GRADIENT_START.2, GRADIENT_END.2, t),
    )
}

pub(super) fn wave_color(
    base: (u8, u8, u8),
    idx: usize,
    len: usize,
    frame_index: usize,
) -> (u8, u8, u8) {
    if len == 0 {
        return base;
    }
    let center = (frame_index as f32 * 1.4) % len as f32;
    let distance = (idx as f32 - center).abs();
    let wrapped = distance.min(len as f32 - distance);
    let glow = (1.0 - (wrapped / 6.0)).clamp(0.0, 1.0);
    blend_toward(base, WAVE_HIGHLIGHT, glow * 0.45)
}

fn lerp(start: u8, end: u8, t: f32) -> u8 {
    (start as f32 + ((end as f32 - start as f32) * t)).round() as u8
}

fn blend_toward(base: (u8, u8, u8), target: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    (
        lerp(base.0, target.0, t),
        lerp(base.1, target.1, t),
        lerp(base.2, target.2, t),
    )
}

fn write_truecolor_text<W: Write>(
    writer: &mut W,
    text: &str,
    (r, g, b): (u8, u8, u8),
) -> std::io::Result<()> {
    write!(writer, "\x1b[38;2;{r};{g};{b}m{text}")
}
