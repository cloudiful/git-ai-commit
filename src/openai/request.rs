use crate::git::RepoContext;
use serde::Serialize;

pub(crate) const SYSTEM_PROMPT: &str = "You write Git commit messages. Output only the commit message text, no code fences, no commentary. Use English Conventional Commit style. Keep the first line within 72 characters. Include a short body only when the change is complex enough to benefit from it. Do not invent behavior not present in the diff.";
pub(crate) const MAX_OUTPUT_TOKENS: usize = 220;

#[derive(Serialize)]
pub(super) struct ResponsesRequest {
    pub(super) model: String,
    pub(super) instructions: String,
    pub(super) input: Vec<ResponseInputMessage>,
    pub(super) temperature: f64,
    pub(super) max_output_tokens: u32,
    pub(super) stream: bool,
}

#[derive(Serialize)]
pub(super) struct ResponseInputMessage {
    pub(super) role: &'static str,
    pub(super) content: String,
}

#[derive(Serialize)]
pub(super) struct ChatCompletionRequest {
    pub(super) model: String,
    pub(super) messages: Vec<ChatMessage>,
    pub(super) temperature: f64,
    pub(super) max_tokens: u32,
    pub(super) stream: bool,
}

#[derive(Serialize)]
pub(super) struct ChatMessage {
    pub(super) role: &'static str,
    pub(super) content: String,
}

pub(super) fn build_prompt(repo_ctx: &RepoContext) -> String {
    let mut prompt = prompt_prefix(
        &repo_ctx.repo_name,
        &repo_ctx.branch_name,
        repo_ctx.changed_file_count,
        repo_ctx.represented_file_count,
    );
    prompt.push_str("Diff coverage: ");
    if repo_ctx.diff_truncated {
        if repo_ctx.diff_budget_is_token_mode {
            prompt.push_str("selective sample within token budget");
        } else {
            prompt.push_str("selective sample within byte budget");
        }
    } else {
        prompt.push_str("full");
    }
    if repo_ctx.diff_stat_truncated {
        prompt.push_str("\nDiff stat coverage: truncated");
    }
    if repo_ctx.secret_redactions > 0 {
        prompt.push_str(&format!(
            "\nSensitive values redacted before prompt: {}",
            repo_ctx.secret_redactions
        ));
    }

    prompt.push_str("\n\nDiff stat:\n");
    if repo_ctx.diff_stat.is_empty() {
        prompt.push_str("(empty)\n");
    } else {
        prompt.push_str(&repo_ctx.diff_stat);
        prompt.push('\n');
    }

    prompt.push_str("\nStaged diff:\n");
    if repo_ctx.diff_patch.is_empty() {
        prompt.push_str("(empty)\n");
    } else {
        prompt.push_str(&repo_ctx.diff_patch);
        prompt.push('\n');
    }

    prompt
}

pub(crate) fn build_prompt_scaffold(
    repo_name: &str,
    branch_name: &str,
    changed_file_count: usize,
) -> String {
    let mut prompt = prompt_prefix(
        repo_name,
        branch_name,
        changed_file_count,
        changed_file_count,
    );
    prompt.push_str("Diff coverage: selective sample within token budget");
    prompt.push_str("\nDiff stat coverage: truncated");
    prompt.push_str("\n\nDiff stat:\n");
    prompt.push_str("\nStaged diff:\n");
    prompt
}

fn prompt_prefix(
    repo_name: &str,
    branch_name: &str,
    changed_file_count: usize,
    represented_file_count: usize,
) -> String {
    let mut prompt = String::from("Generate a commit message from the staged changes.\n\n");
    prompt.push_str(&format!("Repository: {repo_name}\n"));
    prompt.push_str(&format!("Branch: {branch_name}\n"));
    prompt.push_str(&format!("Changed files: {changed_file_count}\n"));
    prompt.push_str(&format!(
        "Represented files in diff sample: {represented_file_count}/{changed_file_count}\n"
    ));
    prompt
}

pub(super) fn responses_url(base: &str) -> String {
    api_endpoint_url(base, "responses")
}

pub(super) fn chat_completions_url(base: &str) -> String {
    api_endpoint_url(base, "chat/completions")
}

fn api_endpoint_url(base: &str, endpoint: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    for known_endpoint in ["/chat/completions", "/responses"] {
        if let Some(prefix) = base.strip_suffix(known_endpoint) {
            return format!("{}/{}", prefix.trim_end_matches('/'), endpoint);
        }
    }
    if base.ends_with("/v1") {
        format!("{base}/{endpoint}")
    } else {
        format!("{base}/v1/{endpoint}")
    }
}

#[cfg(test)]
mod tests {
    use super::{api_endpoint_url, build_prompt, build_prompt_scaffold};
    use crate::git::RepoContext;

    #[test]
    fn rewrites_known_endpoint_urls() {
        assert_eq!(
            api_endpoint_url("https://api.openai.com/v1/chat/completions", "responses"),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            api_endpoint_url("https://api.openai.com/v1/responses", "chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn uses_token_budget_wording_when_requested() {
        let prompt = build_prompt(&RepoContext {
            repo_name: "repo".to_string(),
            branch_name: "main".to_string(),
            diff_truncated: true,
            diff_budget_is_token_mode: true,
            changed_file_count: 3,
            represented_file_count: 2,
            ..RepoContext::default()
        });

        assert!(prompt.contains("Diff coverage: selective sample within token budget"));
    }

    #[test]
    fn scaffold_includes_worst_case_coverage_lines() {
        let prompt = build_prompt_scaffold("repo", "main", 4);

        assert!(prompt.contains("Diff coverage: selective sample within token budget"));
        assert!(prompt.contains("Diff stat coverage: truncated"));
    }
}
