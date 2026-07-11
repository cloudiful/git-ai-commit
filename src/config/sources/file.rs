use super::super::FileConfig;
use super::non_empty_trimmed;
use config::{ConfigSource, ReadOptions, read};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

pub(super) fn load_optional_file_config() -> Result<Option<FileConfig>, String> {
    let Some(path) = std::env::var("GIT_AI_COMMIT_CONFIG_PATH")
        .ok()
        .and_then(|value| non_empty_trimmed(value).map(PathBuf::from))
    else {
        return Ok(None);
    };

    let metadata = std::fs::metadata(&path)
        .map_err(|err| format!("failed to read config file {}: {err}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "failed to read config file {}: path is not a regular file",
            path.display()
        ));
    }

    read(
        &mut PathConfigSource::new(path.clone()),
        Some(ReadOptions::default().without_dotenv()),
    )
    .map(Some)
    .map_err(|err| format!("failed to read config file {}: {err}", path.display()))
}

struct PathConfigSource {
    path: PathBuf,
}

impl PathConfigSource {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ConfigSource for PathConfigSource {
    fn source_name(&self) -> String {
        self.path.display().to_string()
    }

    fn read_value(&mut self) -> io::Result<Option<serde_json::Value>> {
        read_config_value(&self.path).map(Some)
    }

    fn write_config<T>(&mut self, _config: &T) -> io::Result<()>
    where
        T: serde::Serialize,
    {
        Err(io::Error::new(
            ErrorKind::Unsupported,
            format!(
                "writing config through explicit path source is not supported: {}",
                self.path.display()
            ),
        ))
    }
}

fn read_config_value(path: &Path) -> io::Result<serde_json::Value> {
    let content = std::fs::read_to_string(path).map_err(|err| {
        io::Error::new(
            err.kind(),
            format!("failed to read config {}: {err}", path.display()),
        )
    })?;

    match path.extension().and_then(|suffix| suffix.to_str()) {
        Some("toml") => {
            let value: toml::Value = toml::from_str(&content).map_err(|err| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!("failed to parse TOML config {}: {err}", path.display()),
                )
            })?;
            serde_json::to_value(value).map_err(|err| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "failed to convert TOML config {} to JSON value: {err}",
                        path.display()
                    ),
                )
            })
        }
        Some("json") => serde_json::from_str(&content).map_err(|err| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("failed to parse JSON config {}: {err}", path.display()),
            )
        }),
        Some("jsonc") => serde_json::from_str(&strip_jsonc_comments(&content)).map_err(|err| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("failed to parse JSONC config {}: {err}", path.display()),
            )
        }),
        _ => Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!(
                "config file type not supported for {} (expected .toml, .json, or .jsonc)",
                path.display()
            ),
        )),
    }
}

fn strip_jsonc_comments(content: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Normal,
        InString,
        Escaped,
        LineComment,
        BlockComment,
    }

    let mut output = String::with_capacity(content.len());
    let mut state = State::Normal;
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        match state {
            State::Normal if ch == '"' => {
                output.push(ch);
                state = State::InString;
            }
            State::Normal if ch == '/' && matches!(chars.peek(), Some('/')) => {
                output.push_str("  ");
                chars.next();
                state = State::LineComment;
            }
            State::Normal if ch == '/' && matches!(chars.peek(), Some('*')) => {
                output.push_str("  ");
                chars.next();
                state = State::BlockComment;
            }
            State::Normal => output.push(ch),
            State::InString => {
                output.push(ch);
                if ch == '\\' {
                    state = State::Escaped;
                } else if ch == '"' {
                    state = State::Normal;
                }
            }
            State::Escaped => {
                output.push(ch);
                state = State::InString;
            }
            State::LineComment if ch == '\n' => {
                output.push('\n');
                state = State::Normal;
            }
            State::LineComment => output.push(' '),
            State::BlockComment if ch == '*' && matches!(chars.peek(), Some('/')) => {
                output.push_str("  ");
                chars.next();
                state = State::Normal;
            }
            State::BlockComment if ch == '\n' => output.push('\n'),
            State::BlockComment => output.push(' '),
        }
    }
    output
}
