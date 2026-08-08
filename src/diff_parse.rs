#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffFileKind {
    Modified,
    Deleted,
    Added,
    Renamed,
    Copied,
    Binary,
    ModeOnly,
    SubmoduleOrOtherHeaderOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffFile {
    pub header: String,
    pub hunks: Vec<String>,
    pub kind: DiffFileKind,
    pub path: String,
}

pub fn parse_diff_files(raw_diff: &str) -> Vec<DiffFile> {
    if raw_diff.trim().is_empty() {
        return Vec::new();
    }

    let normalized = raw_diff.replace("\r\n", "\n");
    let mut files = Vec::new();
    let mut current: Option<DiffFile> = None;
    let mut current_hunk = String::new();
    let mut in_hunk = false;

    let flush_hunk = |current: &mut Option<DiffFile>, current_hunk: &mut String| {
        if let Some(file) = current.as_mut()
            && !current_hunk.is_empty()
        {
            file.hunks.push(std::mem::take(current_hunk));
        }
    };

    let flush_file = |files: &mut Vec<DiffFile>,
                      current: &mut Option<DiffFile>,
                      current_hunk: &mut String,
                      in_hunk: &mut bool| {
        if current.is_none() {
            return;
        }
        flush_hunk(current, current_hunk);
        let mut file = current.take().expect("current checked");
        file.kind = classify_diff_file(&file);
        files.push(file);
        *in_hunk = false;
    };

    for raw_line in normalized.split_inclusive('\n') {
        if raw_line.starts_with("diff --git ") {
            flush_file(&mut files, &mut current, &mut current_hunk, &mut in_hunk);
            current = Some(DiffFile {
                header: raw_line.to_string(),
                hunks: Vec::new(),
                kind: DiffFileKind::Modified,
                path: parse_target_path(raw_line).unwrap_or_default(),
            });
            continue;
        }

        if current.is_none() {
            current = Some(DiffFile {
                header: String::new(),
                hunks: Vec::new(),
                kind: DiffFileKind::Modified,
                path: String::new(),
            });
        }

        if raw_line.starts_with("@@") {
            flush_hunk(&mut current, &mut current_hunk);
            current_hunk.push_str(raw_line);
            in_hunk = true;
            continue;
        }

        if in_hunk {
            current_hunk.push_str(raw_line);
            continue;
        }

        if let Some(file) = current.as_mut() {
            file.header.push_str(raw_line);
        }
    }

    flush_file(&mut files, &mut current, &mut current_hunk, &mut in_hunk);
    if files.is_empty()
        && let Some(mut file) = current
    {
        file.kind = classify_diff_file(&file);
        files.push(file);
    }

    files
}

fn classify_diff_file(file: &DiffFile) -> DiffFileKind {
    let header = &file.header;
    let has_rename = header.contains("rename from ") && header.contains("rename to ");
    let has_copy = header.contains("copy from ") && header.contains("copy to ");

    if header.contains("GIT binary patch") || header.contains("Binary files ") {
        DiffFileKind::Binary
    } else if has_rename {
        DiffFileKind::Renamed
    } else if has_copy {
        DiffFileKind::Copied
    } else if header.contains("deleted file mode ") {
        DiffFileKind::Deleted
    } else if header.contains("new file mode ") {
        DiffFileKind::Added
    } else if file.hunks.is_empty()
        && (header.contains("old mode ") || header.contains("new mode "))
    {
        DiffFileKind::ModeOnly
    } else if file.hunks.is_empty() {
        DiffFileKind::SubmoduleOrOtherHeaderOnly
    } else {
        DiffFileKind::Modified
    }
}

fn parse_target_path(header_line: &str) -> Option<String> {
    let line = header_line.lines().next()?;
    let rest = line.strip_prefix("diff --git ")?;
    let token = split_diff_path_tokens(rest).nth(1)?;
    token.strip_prefix("b/").map(str::to_string)
}

fn split_diff_path_tokens(input: &str) -> impl Iterator<Item = String> + '_ {
    let mut tokens = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut in_quote = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quote = !in_quote,
            ' ' if !in_quote => {
                if !current.is_empty() {
                    tokens.push(String::from_utf8_lossy(&current).into_owned());
                    current.clear();
                }
            }
            '\\' if in_quote => push_escaped_byte(&mut current, &mut chars),
            _ => push_char_utf8(&mut current, ch),
        }
    }
    if !current.is_empty() {
        tokens.push(String::from_utf8_lossy(&current).into_owned());
    }
    tokens.into_iter()
}

fn push_escaped_byte(bytes: &mut Vec<u8>, chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.next() {
        Some('a') => bytes.push(0x07),
        Some('b') => bytes.push(0x08),
        Some('t') => bytes.push(b'\t'),
        Some('n') => bytes.push(b'\n'),
        Some('v') => bytes.push(0x0b),
        Some('f') => bytes.push(0x0c),
        Some('r') => bytes.push(b'\r'),
        Some('"') => bytes.push(b'"'),
        Some('\\') => bytes.push(b'\\'),
        Some(digit @ '0'..='7') => {
            let mut value = digit.to_digit(8).expect("octal digit");
            for _ in 0..2 {
                let Some(next) = chars.peek().and_then(|c| c.to_digit(8)) else {
                    break;
                };
                value = value * 8 + next;
                chars.next();
            }
            bytes.push(value as u8);
        }
        _ => bytes.push(b'\\'),
    }
}

fn push_char_utf8(bytes: &mut Vec<u8>, ch: char) {
    let mut buf = [0u8; 4];
    bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
}
