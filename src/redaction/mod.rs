use redactor::{InputKind, Redactor, RedactorBuilder};
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactionEntry {
    pub kind: String,
    pub replacement: String,
    pub original: String,
    pub display_value: Option<String>,
    pub occurrences: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RedactionResult {
    pub text: String,
    pub replacement_occurrences: usize,
    pub unique_values: usize,
    pub entries: Vec<RedactionEntry>,
}

pub fn redact_diff_for_prompt(diff: &str) -> RedactionResult {
    if diff.trim().is_empty() {
        return RedactionResult::default();
    }

    match redactor().redact_artifact_with_input_kind(diff, InputKind::GitDiff) {
        Ok(artifact) => RedactionResult {
            text: artifact.result.redacted_text,
            replacement_occurrences: artifact.result.stats.applied_replacements,
            unique_values: artifact.session.entries.len(),
            entries: artifact
                .session
                .entries
                .into_iter()
                .map(|entry| RedactionEntry {
                    kind: entry.kind.label().to_string(),
                    replacement: entry.token,
                    original: entry.original,
                    display_value: entry.replacement_hint,
                    occurrences: entry.occurrences,
                })
                .collect(),
        },
        Err(_) => RedactionResult {
            text: diff.to_string(),
            ..RedactionResult::default()
        },
    }
}

fn redactor() -> &'static Redactor {
    static REDACTOR: OnceLock<Redactor> = OnceLock::new();
    REDACTOR.get_or_init(|| RedactorBuilder::new().build())
}
