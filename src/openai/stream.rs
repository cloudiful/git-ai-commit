use super::StreamOutput;
use super::stream_palette::{StreamPalette, StreamRole};
use super::thinking_status::ThinkingStatus;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub(crate) struct StreamRenderer {
    output: StreamOutput,
    started: bool,
    completed: bool,
    rendered_message: bool,
    palette: StreamPalette,
    in_subject_line: bool,
    in_thinking: bool,
    pending_tag: String,
    output_lock: Arc<Mutex<()>>,
    thinking_status: ThinkingStatus,
}

impl StreamRenderer {
    pub(crate) fn new(output: StreamOutput) -> Self {
        let palette = StreamPalette::detect(output);
        let output_lock = Arc::new(Mutex::new(()));
        Self {
            output,
            started: false,
            completed: false,
            rendered_message: false,
            palette,
            in_subject_line: true,
            in_thinking: false,
            pending_tag: String::new(),
            output_lock: Arc::clone(&output_lock),
            thinking_status: ThinkingStatus::new(output_lock, palette),
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        !matches!(self.output, StreamOutput::None)
    }

    pub(crate) fn push(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() || !self.enabled() {
            return Ok(());
        }

        if !self.started {
            self.start_output()?;
            self.started = true;
        }

        self.clear_thinking_status()?;

        match self.output {
            StreamOutput::Stdout => {
                let output_lock = Arc::clone(&self.output_lock);
                let _guard = output_lock
                    .lock()
                    .map_err(|_| std::io::Error::other("stdout lock poisoned"))?;
                let mut stdout = std::io::stdout().lock();
                self.write_styled(&mut stdout, text)?;
                stdout.flush()
            }
            StreamOutput::None => Ok(()),
        }
    }

    pub(crate) fn push_thinking(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() || !self.enabled() {
            return Ok(());
        }

        if !self.started {
            self.start_output()?;
            self.started = true;
        }

        self.thinking_status.push_text(text)
    }

    pub(crate) fn show_thinking_status(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() || !self.enabled() {
            return Ok(());
        }

        if !self.started {
            self.start_output()?;
            self.started = true;
        }

        self.thinking_status.show_placeholder(text)
    }

    pub(crate) fn finish(&mut self) -> std::io::Result<()> {
        if !self.started || !self.enabled() {
            return Ok(());
        }

        match self.output {
            StreamOutput::Stdout => {
                self.thinking_status.complete()?;
                if self.rendered_message {
                    let _guard = self
                        .output_lock
                        .lock()
                        .map_err(|_| std::io::Error::other("stdout lock poisoned"))?;
                    let mut stdout = std::io::stdout().lock();
                    self.palette.write_reset(&mut stdout)?;
                    writeln!(stdout)?;
                    stdout.flush()?;
                }
            }
            StreamOutput::None => {}
        }
        self.started = false;
        self.completed = self.rendered_message;
        self.in_subject_line = true;
        self.in_thinking = false;
        self.pending_tag.clear();
        self.thinking_status.reset();
        self.rendered_message = false;
        Ok(())
    }

    pub(crate) fn reset(&mut self) {
        let _ = self.abort_thinking_status();
        self.started = false;
        self.completed = false;
        self.rendered_message = false;
        self.in_subject_line = true;
        self.in_thinking = false;
        self.pending_tag.clear();
        self.thinking_status.reset();
    }

    pub(crate) fn completed_render(&self) -> bool {
        self.completed
    }

    fn write_styled<W: Write>(&mut self, writer: &mut W, text: &str) -> std::io::Result<()> {
        const OPEN_TAG: &str = "<think>";
        const CLOSE_TAG: &str = "</think>";

        for ch in text.chars() {
            if self.pending_tag.is_empty() && ch == '<' {
                self.pending_tag.push(ch);
                continue;
            }

            if !self.pending_tag.is_empty() {
                self.pending_tag.push(ch);

                if !self.in_thinking {
                    if OPEN_TAG.starts_with(&self.pending_tag) {
                        if self.pending_tag == OPEN_TAG {
                            self.in_thinking = true;
                            self.pending_tag.clear();
                        }
                        continue;
                    }

                    while !self.pending_tag.is_empty() && !OPEN_TAG.starts_with(&self.pending_tag) {
                        let first = self.pending_tag.remove(0);
                        self.write_char(writer, first)?;
                    }
                    continue;
                }

                if CLOSE_TAG.starts_with(&self.pending_tag) {
                    if self.pending_tag == CLOSE_TAG {
                        self.in_thinking = false;
                        self.pending_tag.clear();
                    }
                    continue;
                }

                let first = self.pending_tag.remove(0);
                self.write_char(writer, first)?;
                continue;
            }

            self.write_char(writer, ch)?;
        }

        Ok(())
    }

    fn clear_thinking_status(&mut self) -> std::io::Result<()> {
        if matches!(self.output, StreamOutput::Stdout) {
            self.thinking_status.complete()
        } else {
            Ok(())
        }
    }

    fn abort_thinking_status(&mut self) -> std::io::Result<()> {
        if matches!(self.output, StreamOutput::Stdout) {
            self.thinking_status.abort()
        } else {
            Ok(())
        }
    }

    fn current_role(&self) -> StreamRole {
        if self.in_thinking {
            StreamRole::Thinking
        } else if self.in_subject_line {
            StreamRole::Subject
        } else {
            StreamRole::Body
        }
    }

    fn write_char<W: Write>(&mut self, writer: &mut W, ch: char) -> std::io::Result<()> {
        self.rendered_message = true;
        self.palette
            .write_style_prefix(writer, self.current_role())?;

        if ch == '\n' {
            writeln!(writer)?;
            self.palette.write_reset(writer)?;
            if !self.in_thinking {
                self.in_subject_line = false;
            }
            return Ok(());
        }

        write!(writer, "{ch}")
    }

    fn start_output(&self) -> std::io::Result<()> {
        match self.output {
            StreamOutput::Stdout => {
                let mut stdout = std::io::stdout().lock();
                stdout.flush()
            }
            StreamOutput::None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamOutput, StreamRenderer};
    use crate::openai::stream_palette::StreamPalette;
    use std::io::BufRead;
    use std::io::Cursor;

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

    fn parse_sse_payloads<R, F>(reader: R, mut on_payload: F) -> Result<(), String>
    where
        R: BufRead,
        F: FnMut(&str) -> Result<bool, String>,
    {
        let mut reader = reader;
        let mut line = String::new();
        let mut data_lines = Vec::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).map_err(|err| err.to_string())?;
            if bytes_read == 0 {
                if !data_lines.is_empty() {
                    let payload = data_lines.join("\n");
                    if !on_payload(&payload)? {
                        return Ok(());
                    }
                }
                return Ok(());
            }

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if !data_lines.is_empty() {
                    let payload = data_lines.join("\n");
                    data_lines.clear();
                    if !on_payload(&payload)? {
                        return Ok(());
                    }
                }
                continue;
            }

            if let Some(payload) = trimmed.strip_prefix("data:") {
                data_lines.push(payload.trim_start().to_string());
            }
        }
    }

    #[test]
    fn parses_sse_payloads() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"feat:\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" add parser\"}}]}\n\n",
            "data: [DONE]\n"
        );
        let mut seen = Vec::new();
        parse_sse_payloads(Cursor::new(body), |payload| {
            seen.push(payload.to_string());
            Ok(payload != "[DONE]")
        })
        .unwrap();
        assert_eq!(
            seen,
            vec![
                "{\"choices\":[{\"delta\":{\"content\":\"feat:\"}}]}".to_string(),
                "{\"choices\":[{\"delta\":{\"content\":\" add parser\"}}]}".to_string(),
                "[DONE]".to_string()
            ]
        );
    }

    #[test]
    fn styles_thinking_sections_and_keeps_subject_color() {
        let mut renderer = StreamRenderer::new(StreamOutput::Stdout);
        renderer.palette = StreamPalette::Ansi;
        let mut out = Vec::new();

        renderer
            .write_styled(&mut out, "<think>drafting</think>feat: add parser\nBody")
            .unwrap();

        let rendered = String::from_utf8(out).unwrap();
        let plain = strip_ansi(&rendered);
        assert!(rendered.contains("\x1b["));
        assert!(plain.contains("drafting"));
        assert!(plain.contains("feat: add parser"));
        assert!(plain.contains("Body"));
        assert!(rendered.contains("\n\x1b[0m"));
    }

    #[test]
    fn handles_split_think_tags_across_chunks() {
        let mut renderer = StreamRenderer::new(StreamOutput::Stdout);
        renderer.palette = StreamPalette::Ansi;
        let mut out = Vec::new();

        renderer.write_styled(&mut out, "<thi").unwrap();
        renderer.write_styled(&mut out, "nk>plan</th").unwrap();
        renderer
            .write_styled(&mut out, "ink>fix: tighten prompt")
            .unwrap();

        let rendered = String::from_utf8(out).unwrap();
        let plain = strip_ansi(&rendered);
        assert!(rendered.contains("\x1b["));
        assert!(plain.contains("plan"));
        assert!(plain.contains("fix: tighten prompt"));
    }

    #[test]
    fn truecolor_palette_uses_lighter_body_tone() {
        let mut renderer = StreamRenderer::new(StreamOutput::Stdout);
        renderer.palette = StreamPalette::TrueColor;
        let mut out = Vec::new();

        renderer
            .write_styled(&mut out, "feat: add parser\nBody")
            .unwrap();

        let rendered = String::from_utf8(out).unwrap();
        assert!(rendered.contains("\x1b[38;2;120;200;255m"));
        assert!(rendered.contains("\x1b[38;2;176;220;255m"));
    }
}
