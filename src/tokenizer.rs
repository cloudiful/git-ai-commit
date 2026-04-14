use tiktoken_rs::{CoreBPE, o200k_base};

#[derive(Clone)]
pub(crate) struct Tokenizer {
    bpe: CoreBPE,
}

impl Tokenizer {
    pub(crate) fn new() -> Result<Self, String> {
        let bpe = o200k_base().map_err(|err| format!("failed to initialize tokenizer: {err}"))?;
        Ok(Self { bpe })
    }

    pub(crate) fn count(&self, input: &str) -> usize {
        self.bpe.encode_with_special_tokens(input).len()
    }

    pub(crate) fn trim_to_budget(&self, input: &str, max_tokens: usize) -> String {
        if max_tokens == 0 {
            return String::new();
        }
        if self.count(input) <= max_tokens {
            return input.to_string();
        }

        let boundaries: Vec<usize> = input
            .char_indices()
            .map(|(idx, _)| idx)
            .chain(std::iter::once(input.len()))
            .collect();
        let mut low = 0usize;
        let mut high = boundaries.len() - 1;
        let mut best = 0usize;

        while low <= high {
            let mid = low + (high - low) / 2;
            let end = boundaries[mid];
            let candidate = &input[..end];
            if self.count(candidate) <= max_tokens {
                best = end;
                low = mid + 1;
            } else if mid == 0 {
                break;
            } else {
                high = mid - 1;
            }
        }

        input[..best].to_string()
    }

    pub(crate) fn trim_with_notice_at_line_boundary(
        &self,
        input: &str,
        max_tokens: usize,
        notice: &str,
    ) -> (String, bool) {
        if max_tokens == 0 {
            return (String::new(), !input.is_empty());
        }
        if self.count(input) <= max_tokens {
            return (input.to_string(), false);
        }

        let notice_tokens = self.count(notice);
        let available = max_tokens.saturating_sub(notice_tokens);
        if available == 0 {
            return (self.trim_to_budget(notice, max_tokens), true);
        }

        let mut trimmed = self.trim_to_budget(input, available);
        if let Some(idx) = trimmed.rfind('\n').filter(|idx| *idx > 0) {
            trimmed.truncate(idx + 1);
        }
        if trimmed.is_empty() {
            trimmed = self.trim_to_budget(input, available);
        }
        if trimmed.is_empty() {
            return (self.trim_to_budget(notice, max_tokens), true);
        }

        (format!("{trimmed}{notice}"), true)
    }
}

#[cfg(test)]
mod tests {
    use super::Tokenizer;

    const NOTICE: &str = "[hunk truncated]\n";

    #[test]
    fn keeps_utf8_valid_when_trimming_tokens() {
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let trimmed = tokenizer.trim_to_budget("你好abc", 1);
        assert!(!trimmed.is_empty());
        assert!(tokenizer.count(&trimmed) <= 1);
        assert!(trimmed.is_char_boundary(trimmed.len()));
    }

    #[test]
    fn prefers_line_boundaries_when_trimming_tokens() {
        let tokenizer = Tokenizer::new().expect("tokenizer");
        let input = "line-1\nline-2\nline-3\nline-4\n";
        let max = tokenizer.count("line-1\nline-2\n") + tokenizer.count(NOTICE);
        let (trimmed, truncated) = tokenizer.trim_with_notice_at_line_boundary(input, max, NOTICE);

        assert!(truncated);
        assert!(trimmed.starts_with("line-1\nline-2\n"));
        assert!(trimmed.ends_with(NOTICE));
    }
}
