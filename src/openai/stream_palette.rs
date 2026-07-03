use super::StreamOutput;
use std::env;
use std::io::Write;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_SUBJECT: &str = "\x1b[1;36m";
const ANSI_BODY: &str = "\x1b[38;5;153m";
const ANSI_THINKING: &str = "\x1b[90m";

const SUBJECT_TRUECOLOR: (u8, u8, u8) = (120, 200, 255);
const BODY_TRUECOLOR: (u8, u8, u8) = (176, 220, 255);
const THINKING_TRUECOLOR: (u8, u8, u8) = (148, 160, 182);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamPalette {
    Plain,
    Ansi,
    TrueColor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamRole {
    Subject,
    Body,
    Thinking,
}

impl StreamPalette {
    pub(crate) fn detect(output: StreamOutput) -> Self {
        if matches!(output, StreamOutput::None) {
            return Self::Plain;
        }

        if env::var_os("NO_COLOR").is_some() {
            return Self::Plain;
        }

        if matches!(env::var("TERM"), Ok(term) if term.eq_ignore_ascii_case("dumb")) {
            return Self::Plain;
        }

        if supports_truecolor() {
            Self::TrueColor
        } else {
            Self::Ansi
        }
    }

    pub(crate) fn colors_enabled(self) -> bool {
        !matches!(self, Self::Plain)
    }

    pub(crate) fn supports_truecolor(self) -> bool {
        matches!(self, Self::TrueColor)
    }

    pub(crate) fn write_style_prefix<W: Write>(
        self,
        writer: &mut W,
        role: StreamRole,
    ) -> std::io::Result<()> {
        match self {
            Self::Plain => Ok(()),
            Self::Ansi => {
                let ansi = match role {
                    StreamRole::Subject => ANSI_SUBJECT,
                    StreamRole::Body => ANSI_BODY,
                    StreamRole::Thinking => ANSI_THINKING,
                };
                write!(writer, "{ansi}")
            }
            Self::TrueColor => {
                let (r, g, b) = match role {
                    StreamRole::Subject => SUBJECT_TRUECOLOR,
                    StreamRole::Body => BODY_TRUECOLOR,
                    StreamRole::Thinking => THINKING_TRUECOLOR,
                };
                write!(writer, "\x1b[38;2;{r};{g};{b}m")
            }
        }
    }

    pub(crate) fn write_reset<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        if self.colors_enabled() {
            write!(writer, "{ANSI_RESET}")
        } else {
            Ok(())
        }
    }
}

fn supports_truecolor() -> bool {
    env::var("COLORTERM")
        .ok()
        .is_some_and(|value| matches_truecolor_hint(&value))
        || env::var("TERM")
            .ok()
            .is_some_and(|value| matches_truecolor_hint(&value))
}

fn matches_truecolor_hint(value: &str) -> bool {
    let lowered = value.trim().to_ascii_lowercase();
    lowered.contains("truecolor") || lowered.contains("24bit") || lowered.contains("direct")
}

#[cfg(test)]
mod tests {
    use super::{StreamPalette, matches_truecolor_hint};

    #[test]
    fn recognizes_truecolor_hints() {
        assert!(matches_truecolor_hint("truecolor"));
        assert!(matches_truecolor_hint("24bit"));
        assert!(matches_truecolor_hint("xterm-direct"));
        assert!(!matches_truecolor_hint("xterm-256color"));
    }

    #[test]
    fn truecolor_palette_reports_color_capabilities() {
        assert!(StreamPalette::TrueColor.colors_enabled());
        assert!(StreamPalette::TrueColor.supports_truecolor());
        assert!(!StreamPalette::Ansi.supports_truecolor());
        assert!(!StreamPalette::Plain.colors_enabled());
    }
}
