use super::StreamOutput;
use std::env;
use std::io::{BufRead, Write};

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_SUBJECT: &str = "\x1b[1;36m";
const ANSI_BODY: &str = "\x1b[2m";

pub(super) fn is_event_stream(content_type: &str) -> bool {
    content_type
        .to_ascii_lowercase()
        .contains("text/event-stream")
}

pub(super) fn parse_sse_payloads<R, F>(reader: R, mut on_payload: F) -> Result<(), String>
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

pub(super) struct StreamRenderer {
    output: StreamOutput,
    started: bool,
    colors_enabled: bool,
    in_subject_line: bool,
}

impl StreamRenderer {
    pub(super) fn new(output: StreamOutput) -> Self {
        Self {
            output,
            started: false,
            colors_enabled: stream_colors_enabled(output),
            in_subject_line: true,
        }
    }

    pub(super) fn enabled(&self) -> bool {
        !matches!(self.output, StreamOutput::None)
    }

    pub(super) fn push(&mut self, text: &str) -> std::io::Result<()> {
        if text.is_empty() || !self.enabled() {
            return Ok(());
        }

        if !self.started {
            match self.output {
                StreamOutput::Stdout => {
                    let mut stdout = std::io::stdout().lock();
                    stdout.flush()?;
                }
                StreamOutput::Stderr => {
                    let mut stderr = std::io::stderr().lock();
                    writeln!(stderr)?;
                    stderr.flush()?;
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
            StreamOutput::Stderr => {
                let mut stderr = std::io::stderr().lock();
                self.write_styled(&mut stderr, text)?;
                stderr.flush()
            }
            StreamOutput::None => Ok(()),
        }
    }

    pub(super) fn finish(&mut self) -> std::io::Result<()> {
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
            StreamOutput::Stderr => {
                let mut stderr = std::io::stderr().lock();
                if self.colors_enabled {
                    write!(stderr, "{ANSI_RESET}")?;
                }
                writeln!(stderr)?;
                stderr.flush()?;
            }
            StreamOutput::None => {}
        }
        self.started = false;
        self.in_subject_line = true;
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        self.started = false;
        self.in_subject_line = true;
    }

    fn write_styled<W: Write>(&mut self, writer: &mut W, text: &str) -> std::io::Result<()> {
        if !self.colors_enabled {
            return write!(writer, "{text}");
        }

        let mut segment_start = 0;
        for (idx, ch) in text.char_indices() {
            if ch != '\n' {
                continue;
            }

            let segment = &text[segment_start..idx];
            if !segment.is_empty() {
                write!(writer, "{}{}", self.current_style(), segment)?;
            }
            writeln!(writer, "{ANSI_RESET}")?;
            self.in_subject_line = false;
            segment_start = idx + 1;
        }

        let tail = &text[segment_start..];
        if !tail.is_empty() {
            write!(writer, "{}{}", self.current_style(), tail)?;
        }

        Ok(())
    }

    fn current_style(&self) -> &'static str {
        if self.in_subject_line {
            ANSI_SUBJECT
        } else {
            ANSI_BODY
        }
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
    use super::parse_sse_payloads;
    use std::io::Cursor;

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
}
