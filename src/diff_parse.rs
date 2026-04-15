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
            });
            continue;
        }

        if current.is_none() {
            current = Some(DiffFile {
                header: String::new(),
                hunks: Vec::new(),
                kind: DiffFileKind::Modified,
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
    if files.is_empty() && let Some(mut file) = current {
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
