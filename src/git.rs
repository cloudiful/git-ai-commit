use crate::config::Config;
use crate::diff_parse::parse_diff_files;
use crate::diff_sampling::{DiffBudget, prepare_diff_for_prompt, resolve_diff_budget};
use crate::redaction::{RedactionEntry, RedactionResult, redact_diff_for_prompt};
use std::path::Path;
use std::process::{Command, Stdio};

const REDACTION_PREVIEW_LIMIT: usize = 8;
const REDACTION_PREVIEW_VALUE_CHARS: usize = 72;

#[derive(Clone, Debug, Default)]
pub struct RepoContext {
    pub repo_name: String,
    pub branch_name: String,
    pub diff_stat: String,
    pub diff_patch: String,
    pub diff_truncated: bool,
    pub diff_stat_truncated: bool,
    pub diff_budget_is_token_mode: bool,
    pub secret_redactions: usize,
    pub secret_redaction_preview: String,
    pub changed_file_count: usize,
    pub represented_file_count: usize,
}

pub fn collect_repo_context(cfg: &Config) -> Result<RepoContext, String> {
    let repo_root = std::env::var("GIT_AI_COMMIT_REPO_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            run_git(None::<&Path>, ["rev-parse", "--show-toplevel"]).unwrap_or_default()
        })
        .trim()
        .to_string();

    if repo_root.is_empty() {
        return Err("git rev-parse --show-toplevel failed".to_string());
    }

    let repo_path = Path::new(&repo_root);
    let branch_name = current_branch(repo_path)?;
    let diff_stat = run_git(
        Some(repo_path),
        ["diff", "--cached", "--stat", "--no-ext-diff"],
    )?;
    let diff_patch = run_git(
        Some(repo_path),
        ["diff", "--cached", "--no-ext-diff", "--unified=3"],
    )?;
    let files = parse_diff_files(&diff_patch);
    let repo_name = repo_path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo_root.clone());
    let budget = resolve_diff_budget(cfg.diff_budget(), &repo_name, &branch_name, files.len())?;
    let redacted_diff = if cfg.redact_secrets {
        redact_diff_for_prompt(&diff_patch, cfg.redaction_rules)
    } else {
        RedactionResult {
            text: diff_patch,
            replacement_occurrences: 0,
            unique_values: 0,
            entries: Vec::new(),
        }
    };
    let (diff_stat, diff_patch, sampling) =
        prepare_diff_for_prompt(&files, &diff_stat, &redacted_diff.text, budget)?;

    if sampling.sampled {
        log_sampling_notice(budget, sampling.represented_files, sampling.total_files);
    }
    if redacted_diff.replacement_occurrences > 0 {
        if redacted_diff.unique_values > 0 {
            eprintln!(
                "git-ai-commit: redacted {} unique sensitive-looking value(s) across {} occurrence(s) before sending the diff to the model",
                redacted_diff.unique_values, redacted_diff.replacement_occurrences
            );
        } else {
            eprintln!(
                "git-ai-commit: redacted {} sensitive-looking occurrence(s) before sending the diff to the model",
                redacted_diff.replacement_occurrences
            );
        }
    }

    Ok(RepoContext {
        repo_name,
        branch_name,
        diff_stat: diff_stat.trim().to_string(),
        diff_patch: diff_patch.trim().to_string(),
        diff_truncated: sampling.sampled,
        diff_stat_truncated: sampling.stat_truncated,
        diff_budget_is_token_mode: budget.is_token_mode(),
        secret_redactions: redacted_diff.replacement_occurrences,
        secret_redaction_preview: format_redaction_preview(&redacted_diff.entries),
        changed_file_count: sampling.total_files,
        represented_file_count: sampling.represented_files,
    })
}

fn format_redaction_preview(entries: &[RedactionEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut preview =
        String::from("git-ai-commit: redaction preview before sending the diff to the model:\n");

    for entry in entries.iter().take(REDACTION_PREVIEW_LIMIT) {
        preview.push_str("  - ");
        preview.push_str(&entry.kind);
        preview.push(' ');
        preview.push_str(&entry.replacement);
        if entry.occurrences > 1 {
            preview.push_str(&format!(" x{}", entry.occurrences));
        }
        preview.push_str(" <= ");
        preview.push_str(&preview_value(&entry.original));
        if let Some(display_value) = entry
            .display_value
            .as_ref()
            .filter(|value| *value != &entry.original)
        {
            preview.push_str(" (hint ");
            preview.push_str(display_value);
            preview.push(')');
        }
        preview.push('\n');
    }

    if entries.len() > REDACTION_PREVIEW_LIMIT {
        preview.push_str(&format!(
            "git-ai-commit: ... and {} more redacted value(s)\n",
            entries.len() - REDACTION_PREVIEW_LIMIT
        ));
    }

    preview
}

fn preview_value(value: &str) -> String {
    let escaped = value.escape_default().to_string();
    let total_chars = escaped.chars().count();
    let truncated = escaped
        .chars()
        .take(REDACTION_PREVIEW_VALUE_CHARS)
        .collect::<String>();
    if total_chars > REDACTION_PREVIEW_VALUE_CHARS {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn log_sampling_notice(budget: DiffBudget, represented_files: usize, total_files: usize) {
    match budget {
        DiffBudget::Bytes { max_bytes } => eprintln!(
            "git-ai-commit: staged diff selectively sampled within {} byte budget ({}/{}) files represented",
            max_bytes, represented_files, total_files
        ),
        DiffBudget::Tokens {
            configured_tokens,
            effective_tokens,
        } if configured_tokens != effective_tokens => eprintln!(
            "git-ai-commit: staged diff selectively sampled within configured {} tokens, effective {} tokens after context clamp ({}/{}) files represented",
            configured_tokens, effective_tokens, represented_files, total_files
        ),
        DiffBudget::Tokens {
            configured_tokens, ..
        } => eprintln!(
            "git-ai-commit: staged diff selectively sampled within configured {} tokens ({}/{}) files represented",
            configured_tokens, represented_files, total_files
        ),
    }
}

pub fn current_branch(repo_root: &Path) -> Result<String, String> {
    match run_git(
        Some(repo_root),
        ["symbolic-ref", "--quiet", "--short", "HEAD"],
    ) {
        Ok(value) => Ok(value.trim().to_string()),
        Err(_) => {
            let short = run_git(Some(repo_root), ["rev-parse", "--short", "HEAD"])?;
            Ok(format!("detached-{}", short.trim()))
        }
    }
}

pub fn run_git<I, S>(repo_root: Option<&Path>, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arg_vec: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    let mut command = Command::new("git");
    command.args(&arg_vec);
    if let Some(repo_root) = repo_root {
        command.current_dir(repo_root);
    }

    let output = command.output().map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            arg_vec.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

pub fn run_git_interactive<I, S>(repo_root: Option<&Path>, args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arg_vec: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    let mut command = Command::new("git");
    command.args(&arg_vec);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    if let Some(repo_root) = repo_root {
        command.current_dir(repo_root);
    }

    let status = command.status().map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed with status {}",
            arg_vec.join(" "),
            status
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTION_PREVIEW_VALUE_CHARS, format_redaction_preview};
    use crate::redaction::RedactionEntry;

    #[test]
    fn formats_redaction_preview_with_occurrences_and_hint() {
        let preview = format_redaction_preview(&[
            RedactionEntry {
                kind: "secret".to_string(),
                replacement: "__R_SECRET_001__".to_string(),
                original: "sk_live_1234567890ABCDEF".to_string(),
                display_value: Some("<secret>".to_string()),
                occurrences: 2,
            },
            RedactionEntry {
                kind: "domain".to_string(),
                replacement: "__R_DOMAIN_001__".to_string(),
                original: "prod.internal.example.com".to_string(),
                display_value: Some("prod.internal.example.com".to_string()),
                occurrences: 1,
            },
        ]);

        assert!(preview.contains("redaction preview before sending the diff"));
        assert!(preview.contains("secret __R_SECRET_001__ x2 <="));
        assert!(preview.contains("sk_live_1234567890ABCDEF"));
        assert!(preview.contains("(hint <secret>)"));
        assert!(preview.contains("domain __R_DOMAIN_001__ <= prod.internal.example.com"));
    }

    #[test]
    fn truncates_long_values_and_reports_overflow_entries() {
        let preview = format_redaction_preview(
            &(0..9)
                .map(|idx| RedactionEntry {
                    kind: "url".to_string(),
                    replacement: format!("__R_URL_{idx:03}__"),
                    original: format!(
                        "https://example.com/{}",
                        "a".repeat(REDACTION_PREVIEW_VALUE_CHARS + 20)
                    ),
                    display_value: None,
                    occurrences: 1,
                })
                .collect::<Vec<_>>(),
        );

        assert!(preview.contains("https://example.com/"));
        assert!(preview.contains("..."));
        assert!(preview.contains("... and 1 more redacted value(s)"));
    }
}
