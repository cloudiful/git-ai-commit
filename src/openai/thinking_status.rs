use super::stream_palette::StreamPalette;
use std::io::Write;

const ANSI_CLEAR_LINE: &str = "\x1b[2K";
const ANSI_THINKING: &str = "\x1b[90m";
const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const LABEL: &str = "thinking";
const STATUS_PREVIEW_LIMIT: usize = 80;
const GRADIENT_START: (u8, u8, u8) = (142, 148, 163);
const GRADIENT_END: (u8, u8, u8) = (110, 198, 214);

#[derive(Default)]
pub(crate) struct ThinkingStatus {
    active: bool,
    text: String,
    frame: usize,
    has_reasoning_text: bool,
}

impl ThinkingStatus {
    pub(crate) fn show_placeholder(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
        self.has_reasoning_text = false;
    }

    pub(crate) fn push_text(&mut self, text: &str) {
        if !self.has_reasoning_text {
            self.text.clear();
            self.has_reasoning_text = true;
        }
        self.text.push_str(text);
    }

    pub(crate) fn render<W: Write>(
        &mut self,
        writer: &mut W,
        palette: StreamPalette,
    ) -> std::io::Result<()> {
        self.clear(writer)?;
        self.active = true;

        let frame_index = self.frame;
        let frame = SPINNER_FRAMES[frame_index % SPINNER_FRAMES.len()];
        self.frame = self.frame.wrapping_add(1);

        let mut line = String::from(LABEL);
        let preview = preview_text(&self.text);
        if !preview.is_empty() {
            line.push_str(": ");
            line.push_str(&preview);
        }

        write!(writer, "\r")?;
        if palette.supports_truecolor() {
            write_gradient(writer, frame, &line, frame_index)?;
            palette.write_reset(writer)?;
        } else if palette.colors_enabled() {
            write!(writer, "{ANSI_THINKING}{frame} {line}")?;
            palette.write_reset(writer)?;
        } else {
            write!(writer, "{frame} {line}")?;
        }
        Ok(())
    }

    pub(crate) fn clear<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        if !self.active {
            return Ok(());
        }

        write!(writer, "\r{ANSI_CLEAR_LINE}\r")?;
        self.active = false;
        Ok(())
    }

    pub(crate) fn reset(&mut self) {
        self.active = false;
        self.text.clear();
        self.frame = 0;
        self.has_reasoning_text = false;
    }
}

fn preview_text(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let preview = chars
        .by_ref()
        .take(STATUS_PREVIEW_LIMIT)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

fn write_gradient<W: Write>(
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

fn wave_color(base: (u8, u8, u8), idx: usize, len: usize, frame_index: usize) -> (u8, u8, u8) {
    if len == 0 {
        return base;
    }

    let wave_center = (frame_index as f32 * 1.4) % len as f32;
    let raw_distance = (idx as f32 - wave_center).abs();
    let wrapped_distance = raw_distance.min(len as f32 - raw_distance);
    let glow = (1.0 - (wrapped_distance / 6.0)).clamp(0.0, 1.0);
    blend_toward(base, (230, 246, 255), glow * 0.45)
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
    use super::{StreamPalette, ThinkingStatus, preview_text};

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
    fn preview_compacts_and_truncates() {
        let preview = preview_text(
            "considering   staged\n\nchanges and summarizing the diff in a compact way for terminal display",
        );

        assert!(preview.starts_with("considering staged changes"));
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn render_writes_unicode_spinner_and_truecolor_gradient() {
        let mut status = ThinkingStatus::default();
        status.push_text("considering diff");
        let mut out = Vec::new();

        status.render(&mut out, StreamPalette::TrueColor).unwrap();

        let rendered = String::from_utf8(out).unwrap();
        let plain = strip_ansi(&rendered);
        assert!(rendered.contains("⠋"));
        assert!(rendered.contains("\x1b[38;2;"));
        assert!(plain.contains("thinking: considering diff"));
    }

    #[test]
    fn placeholder_is_replaced_when_reasoning_text_arrives() {
        let mut status = ThinkingStatus::default();
        status.show_placeholder("drafting commit message");
        status.push_text("considering diff");
        let mut out = Vec::new();

        status.render(&mut out, StreamPalette::Plain).unwrap();

        let rendered = String::from_utf8(out).unwrap();
        assert!(rendered.contains("thinking: considering diff"));
        assert!(!rendered.contains("drafting commit message"));
    }
}
