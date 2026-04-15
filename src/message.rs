use crate::text_budget;

pub const MAX_SUBJECT_CHARS: usize = 72;

pub fn sanitize_message(message: &str) -> String {
    let normalized = message.replace("\r\n", "\n");
    let trimmed = normalized
        .trim()
        .trim_start_matches("```")
        .trim_end_matches("```");

    trimmed
        .trim()
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
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

#[cfg(test)]
mod tests {
    use super::{sanitize_message, trim_to_utf8_bytes, trim_with_notice_at_line_boundary};

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
}
