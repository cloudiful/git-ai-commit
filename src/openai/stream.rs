use super::StreamOutput;
use std::env;
use std::io::Write;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_SUBJECT: &str = "\x1b[1;36m";
const ANSI_BODY: &str = "\x1b[39m";
const ANSI_THINKING: &str = "\x1b[90m";

pub(crate) struct StreamRenderer {
    output: StreamOutput,
    started: bool,
    completed: bool,
    colors_enabled: bool,
    in_subject_line: bool,
    in_thinking: bool,
    pending_tag: String,
}

impl StreamRenderer {
    pub(crate) fn new(output: StreamOutput) -> Self {
        Self {
            output,
            started: false,
            completed: false,
            colors_enabled: stream_colors_enabled(output),
            in_subject_line: true,
            in_thinking: false,
            pending_tag: String::new(),
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
            match self.output {
                StreamOutput::Stdout => {
                    let mut stdout = std::io::stdout().lock();
                    stdout.flush()?;
                }
                StreamOutput::None => {}
            }
            self.started = true;
        }

        match self.output {
            StreamOutput::Stdout => {
                let mut stdout = std::io::stdout().lock();
                self.write_styled(&mut stdout, text)?;
                stdout.flush()
            }
            StreamOutput::None => Ok(()),
        }
    }

    pub(crate) fn finish(&mut self) -> std::io::Result<()> {
        if !self.started || !self.enabled() {
            return Ok(());
        }

        match self.output {
            StreamOutput::Stdout => {
                let mut stdout = std::io::stdout().lock();
                if self.colors_enabled {
                    write!(stdout, "{ANSI_RESET}")?;
                }
                writeln!(stdout)?;
                stdout.flush()?;
            }
            StreamOutput::None => {}
        }
        self.started = false;
        self.completed = true;
        self.in_subject_line = true;
        self.in_thinking = false;
        self.pending_tag.clear();
        Ok(())
    }

    pub(crate) fn reset(&mut self) {
        self.started = false;
        self.completed = false;
        self.in_subject_line = true;
        self.in_thinking = false;
        self.pending_tag.clear();
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

    fn current_style(&self) -> &'static str {
        if self.in_thinking {
            ANSI_THINKING
        } else if self.in_subject_line {
            ANSI_SUBJECT
        } else {
            ANSI_BODY
        }
    }

    fn write_char<W: Write>(&mut self, writer: &mut W, ch: char) -> std::io::Result<()> {
        if self.colors_enabled {
            write!(writer, "{}", self.current_style())?;
        }

        if ch == '\n' {
            writeln!(writer)?;
            if self.colors_enabled {
                write!(writer, "{ANSI_RESET}")?;
            }
            if !self.in_thinking {
                self.in_subject_line = false;
            }
            return Ok(());
        }

        write!(writer, "{ch}")
    }
}

fn stream_colors_enabled(output: StreamOutput) -> bool {
    if matches!(output, StreamOutput::None) {
        return false;
    }

    if env::var_os("NO_COLOR").is_some() {
        return false;
    }

    if matches!(env::var("TERM"), Ok(term) if term.eq_ignore_ascii_case("dumb")) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{ANSI_BODY, ANSI_RESET, ANSI_SUBJECT, ANSI_THINKING, StreamOutput, StreamRenderer};
    use std::io::BufRead;
    use std::io::Cursor;

    fn strip_known_ansi(input: &str) -> String {
        input
            .replace(ANSI_THINKING, "")
            .replace(ANSI_SUBJECT, "")
            .replace(ANSI_BODY, "")
            .replace(ANSI_RESET, "")
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
        renderer.colors_enabled = true;
        let mut out = Vec::new();

        renderer
            .write_styled(&mut out, "<think>drafting</think>feat: add parser\nBody")
            .unwrap();

        let rendered = String::from_utf8(out).unwrap();
        let plain = strip_known_ansi(&rendered);
        assert!(rendered.contains(ANSI_THINKING));
        assert!(rendered.contains(ANSI_SUBJECT));
        assert!(rendered.contains(ANSI_BODY));
        assert!(plain.contains("drafting"));
        assert!(plain.contains("feat: add parser"));
        assert!(plain.contains("Body"));
        assert!(rendered.contains(&format!("\n{ANSI_RESET}")));
    }

    #[test]
    fn handles_split_think_tags_across_chunks() {
        let mut renderer = StreamRenderer::new(StreamOutput::Stdout);
        renderer.colors_enabled = true;
        let mut out = Vec::new();

        renderer.write_styled(&mut out, "<thi").unwrap();
        renderer.write_styled(&mut out, "nk>plan</th").unwrap();
        renderer
            .write_styled(&mut out, "ink>fix: tighten prompt")
            .unwrap();

        let rendered = String::from_utf8(out).unwrap();
        let plain = strip_known_ansi(&rendered);
        assert!(rendered.contains(ANSI_THINKING));
        assert!(rendered.contains(ANSI_SUBJECT));
        assert!(plain.contains("plan"));
        assert!(plain.contains("fix: tighten prompt"));
    }
}
