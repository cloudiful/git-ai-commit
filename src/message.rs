use crate::text_budget;

pub const MAX_SUBJECT_CHARS: usize = 72;

pub fn sanitize_message(message: &str) -> String {
    let normalized = message.replace("\r\n", "\n");
    let trimmed = normalized
        .trim()
        .trim_start_matches("```")
        .trim_end_matches("```");

    let sanitized = trimmed
        .trim()
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    enforce_subject_limit(&sanitized)
}

pub fn validate_message(message: &str) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("model returned an empty message".to_string());
    }

    let subject_len = first_line(message).chars().count();
    if subject_len > MAX_SUBJECT_CHARS {
        return Err(format!(
            "generated subject exceeds {MAX_SUBJECT_CHARS} characters"
        ));
    }

    Ok(())
}

pub fn trim_to_utf8_bytes(input: &str, max_bytes: usize) -> String {
    if max_bytes == 0 {
        return String::new();
    }
    if input.len() <= max_bytes {
        return input.to_string();
    }

    let mut end = max_bytes;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    input[..end].to_string()
}

pub fn trim_with_notice_at_line_boundary(
    input: &str,
    max_bytes: usize,
    notice: &str,
) -> (String, bool) {
    text_budget::trim_with_notice_at_line_boundary(
        input,
        max_bytes,
        notice,
        |value| value.len(),
        trim_to_utf8_bytes,
    )
}

pub fn first_line(input: &str) -> &str {
    input.split('\n').next().unwrap_or(input)
}

fn enforce_subject_limit(message: &str) -> String {
    if message.trim().is_empty() {
        return String::new();
    }

    let mut lines = message.lines().map(str::to_string).collect::<Vec<_>>();
    if let Some(subject) = lines.first_mut()
        && subject.chars().count() > MAX_SUBJECT_CHARS
    {
        *subject = truncate_subject(subject);
    }

    lines.join("\n").trim().to_string()
}

fn truncate_subject(subject: &str) -> String {
    if subject.chars().count() <= MAX_SUBJECT_CHARS {
        return subject.to_string();
    }

    let mut cutoff = 0;
    let mut last_space_cutoff = None;

    for (idx, ch) in subject.char_indices() {
        let next = idx + ch.len_utf8();
        if subject[..next].chars().count() > MAX_SUBJECT_CHARS {
            break;
        }
        cutoff = next;
        if ch.is_whitespace() {
            last_space_cutoff = Some(idx);
        }
    }

    let preferred = last_space_cutoff.unwrap_or(cutoff);
    subject[..preferred].trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SUBJECT_CHARS, sanitize_message, trim_to_utf8_bytes, trim_with_notice_at_line_boundary,
    };

    const DIFF_HUNK_TRUNCATED_NOTICE: &str = "[hunk truncated]\n";

    #[test]
    fn keeps_utf8_valid_when_trimming() {
        let trimmed = trim_to_utf8_bytes("你好abc", 5);
        assert_eq!(trimmed, "你");
        assert!(trimmed.is_char_boundary(trimmed.len()));
    }

    #[test]
    fn prefers_line_boundaries() {
        let input = "line-1\nline-2\nline-3\nline-4\nline-5\n";
        let max = "line-1\nline-2\n".len() + DIFF_HUNK_TRUNCATED_NOTICE.len();
        let (trimmed, truncated) =
            trim_with_notice_at_line_boundary(input, max, DIFF_HUNK_TRUNCATED_NOTICE);

        assert!(truncated);
        assert!(trimmed.starts_with("line-1\nline-2\n"));
        assert!(trimmed.ends_with(DIFF_HUNK_TRUNCATED_NOTICE));
    }

    #[test]
    fn falls_back_to_notice_for_tiny_budget() {
        let (trimmed, truncated) =
            trim_with_notice_at_line_boundary("abcdef", 4, DIFF_HUNK_TRUNCATED_NOTICE);

        assert!(truncated);
        assert!(!trimmed.is_empty());
        assert!(trimmed.len() <= 4);
    }

    #[test]
    fn removes_code_fences() {
        let sanitized = sanitize_message("```text\nfeat: add tests\n\nbody\n```");
        assert!(!sanitized.contains("```"));
        assert!(sanitized.contains("feat: add tests"));
    }

    #[test]
    fn truncates_overlong_subject_at_word_boundary() {
        let sanitized = sanitize_message(
            "feat: add OpenRouter model context auto-detection and provider debug logging\n\n- body",
        );
        assert!(super::first_line(&sanitized).chars().count() <= MAX_SUBJECT_CHARS);
        assert_eq!(
            super::first_line(&sanitized),
            "feat: add OpenRouter model context auto-detection and provider"
        );
        assert!(sanitized.contains("\n\n- body"));
    }
}
