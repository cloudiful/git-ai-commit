use super::stream_palette::StreamPalette;
use std::io::Write;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const ANSI_CLEAR_LINE: &str = "\x1b[2K";
const ANSI_THINKING: &str = "\x1b[90m";
const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const LABEL: &str = "thinking";
const STATUS_WINDOW_WIDTH: usize = 56;
const STATUS_SCROLL_GAP: usize = 8;
const FRAME_INTERVAL: Duration = Duration::from_millis(80);
const GRADIENT_START: (u8, u8, u8) = (142, 148, 163);
const GRADIENT_END: (u8, u8, u8) = (110, 198, 214);
const WAVE_HIGHLIGHT: (u8, u8, u8) = (230, 246, 255);

pub(crate) struct ThinkingStatus {
    palette: StreamPalette,
    output_lock: Arc<Mutex<()>>,
    worker: Option<ThinkingStatusWorker>,
}

struct ThinkingStatusWorker {
    tx: mpsc::Sender<ThinkingCommand>,
    handle: JoinHandle<()>,
}

enum ThinkingCommand {
    SetPlaceholder(String),
    AppendReasoning(String),
    Stop,
}

struct ThinkingState {
    text: String,
    has_reasoning_text: bool,
}

impl ThinkingStatus {
    pub(crate) fn new(output_lock: Arc<Mutex<()>>, palette: StreamPalette) -> Self {
        Self {
            palette,
            output_lock,
            worker: None,
        }
    }

    pub(crate) fn show_placeholder(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        self.ensure_worker()?;
        self.send_command(ThinkingCommand::SetPlaceholder(text.to_string()))
    }

    pub(crate) fn push_text(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        self.ensure_worker()?;
        self.send_command(ThinkingCommand::AppendReasoning(text.to_string()))
    }

    pub(crate) fn stop(&mut self) -> std::io::Result<()> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };

        let _ = worker.tx.send(ThinkingCommand::Stop);
        worker
            .handle
            .join()
            .map_err(|_| std::io::Error::other("thinking status worker panicked"))?;
        self.clear_line()
    }

    pub(crate) fn reset(&mut self) {
        let _ = self.stop();
    }

    fn ensure_worker(&mut self) -> std::io::Result<()> {
        if self.worker.is_some() {
            return Ok(());
        }

        let (tx, rx) = mpsc::channel();
        let output_lock = Arc::clone(&self.output_lock);
        let palette = self.palette;
        let handle = thread::Builder::new()
            .name("git-ai-commit-thinking".to_string())
            .spawn(move || {
                run_worker(rx, output_lock, palette);
            })
            .map_err(std::io::Error::other)?;

        self.worker = Some(ThinkingStatusWorker { tx, handle });
        Ok(())
    }

    fn send_command(&self, command: ThinkingCommand) -> std::io::Result<()> {
        let Some(worker) = &self.worker else {
            return Ok(());
        };

        worker.tx.send(command).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "thinking status worker stopped",
            )
        })
    }

    fn clear_line(&self) -> std::io::Result<()> {
        let _guard = self
            .output_lock
            .lock()
            .map_err(|_| std::io::Error::other("stdout lock poisoned"))?;
        let mut stdout = std::io::stdout().lock();
        write!(stdout, "\r{ANSI_CLEAR_LINE}\r")?;
        stdout.flush()
    }
}

fn run_worker(
    rx: mpsc::Receiver<ThinkingCommand>,
    output_lock: Arc<Mutex<()>>,
    palette: StreamPalette,
) {
    let mut state = ThinkingState {
        text: String::new(),
        has_reasoning_text: false,
    };
    let mut frame_index = 0usize;
    let mut active = false;

    loop {
        match rx.recv_timeout(FRAME_INTERVAL) {
            Ok(ThinkingCommand::SetPlaceholder(text)) => {
                state.text = text;
                state.has_reasoning_text = false;
                active = render_status(&output_lock, palette, frame_index, &state.text).is_ok();
                frame_index = frame_index.wrapping_add(1);
            }
            Ok(ThinkingCommand::AppendReasoning(delta)) => {
                if !state.has_reasoning_text {
                    state.text.clear();
                    state.has_reasoning_text = true;
                }
                state.text.push_str(&delta);
                active = render_status(&output_lock, palette, frame_index, &state.text).is_ok();
                frame_index = frame_index.wrapping_add(1);
            }
            Ok(ThinkingCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if active {
                    let _ = clear_status_line(&output_lock);
                }
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if state.text.is_empty() {
                    continue;
                }
                active = render_status(&output_lock, palette, frame_index, &state.text).is_ok();
                frame_index = frame_index.wrapping_add(1);
            }
        }
    }
}

fn render_status(
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

fn clear_status_line(output_lock: &Arc<Mutex<()>>) -> std::io::Result<()> {
    let _guard = output_lock
        .lock()
        .map_err(|_| std::io::Error::other("stdout lock poisoned"))?;
    let mut stdout = std::io::stdout().lock();
    write!(stdout, "\r{ANSI_CLEAR_LINE}\r")?;
    stdout.flush()
}

fn write_status_line<W: Write>(
    writer: &mut W,
    palette: StreamPalette,
    frame_index: usize,
    text: &str,
) -> std::io::Result<()> {
    let frame = SPINNER_FRAMES[frame_index % SPINNER_FRAMES.len()];
    let mut line = String::from(LABEL);
    let window = scrolling_window_text(text, frame_index);
    if !window.is_empty() {
        line.push_str(": ");
        line.push_str(&window);
    }

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

fn scrolling_window_text(text: &str, frame_index: usize) -> String {
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
    let scroll_step = frame_index / 2;
    let start = scroll_step % marquee.len();

    marquee
        .iter()
        .cycle()
        .skip(start)
        .take(STATUS_WINDOW_WIDTH)
        .collect()
}

fn pad_right(mut text: String, width: usize) -> String {
    let len = text.chars().count();
    if len < width {
        text.extend(std::iter::repeat_n(' ', width - len));
    }
    text
}

fn write_wave_gradient<W: Write>(
    writer: &mut W,
    frame: &str,
    text: &str,
    frame_index: usize,
) -> std::io::Result<()> {
    let gradient_chars = text.chars().collect::<Vec<_>>();
    write_truecolor_text(writer, frame, GRADIENT_END)?;
    write!(writer, " ")?;

    for (idx, ch) in gradient_chars.iter().enumerate() {
        let color = wave_color(
            interpolate_color(idx, gradient_chars.len()),
            idx,
            gradient_chars.len(),
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

    let scale = (len - 1) as f32;
    let t = idx as f32 / scale;
    (
        lerp(GRADIENT_START.0, GRADIENT_END.0, t),
        lerp(GRADIENT_START.1, GRADIENT_END.1, t),
        lerp(GRADIENT_START.2, GRADIENT_END.2, t),
    )
}

fn wave_color(base: (u8, u8, u8), idx: usize, len: usize, frame_index: usize) -> (u8, u8, u8) {
    if len == 0 {
        return base;
    }

    let wave_center = (frame_index as f32 * 1.4) % len as f32;
    let raw_distance = (idx as f32 - wave_center).abs();
    let wrapped_distance = raw_distance.min(len as f32 - raw_distance);
    let glow = (1.0 - (wrapped_distance / 6.0)).clamp(0.0, 1.0);
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

#[cfg(test)]
mod tests {
    use super::{
        StreamPalette, ThinkingState, scrolling_window_text, wave_color, write_status_line,
    };

    fn strip_ansi(input: &str) -> String {
        let mut out = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            out.push(ch);
        }

        out
    }

    #[test]
    fn short_status_text_is_padded_to_fixed_width() {
        let status = scrolling_window_text("considering diff", 0);

        assert_eq!(status.chars().count(), 56);
        assert!(status.starts_with("considering diff"));
    }

    #[test]
    fn long_status_text_scrolls_over_time() {
        let status_a = scrolling_window_text(
            "considering staged changes and summarizing the diff in a compact way for terminal display",
            0,
        );
        let status_b = scrolling_window_text(
            "considering staged changes and summarizing the diff in a compact way for terminal display",
            24,
        );

        assert_eq!(status_a.chars().count(), 56);
        assert_eq!(status_b.chars().count(), 56);
        assert_ne!(status_a, status_b);
    }

    #[test]
    fn write_status_line_renders_unicode_spinner_and_truecolor_gradient() {
        let mut out = Vec::new();

        write_status_line(&mut out, StreamPalette::TrueColor, 0, "considering diff").unwrap();

        let rendered = String::from_utf8(out).unwrap();
        let plain = strip_ansi(&rendered);
        assert!(rendered.contains("⠋"));
        assert!(rendered.contains("\x1b[38;2;"));
        assert!(plain.contains("thinking: considering diff"));
    }

    #[test]
    fn placeholder_is_replaced_when_reasoning_text_arrives() {
        let mut state = ThinkingState {
            text: "drafting commit message".to_string(),
            has_reasoning_text: false,
        };

        if !state.has_reasoning_text {
            state.text.clear();
            state.has_reasoning_text = true;
        }
        state.text.push_str("considering diff");

        assert_eq!(state.text, "considering diff");
    }

    #[test]
    fn wave_highlight_moves_between_frames() {
        let base = (150, 170, 190);
        let color_a = wave_color(base, 3, 20, 0);
        let color_b = wave_color(base, 3, 20, 6);

        assert_ne!(color_a, color_b);
    }
}
