use crate::git::RepoContext;
use serde::Serialize;
use serde_json::Value;

pub(crate) const SYSTEM_PROMPT: &str = "You are an expert at writing Git commit messages. Your job is to write a short, clear commit message that summarizes the staged changes.\n\nUse English Conventional Commit style for the subject line.\n\nIf you can accurately express the change in just the subject line, do not include a message body. Only use the body when it provides useful information. Do not repeat information from the subject line in the body.\n\nReturn only the final commit message. Do not explain your reasoning. Do not describe the task. Do not preface the answer. Do not include code fences. Do not include the raw diff in the commit message.\n\nFollow good Git style:\n- Separate the subject from the body with a blank line\n- Keep the subject line within 72 characters\n- Use the imperative mood in the subject line\n- Keep the body short and concise\n- Do not invent behavior not present in the diff";
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) response_format: Option<ChatResponseFormat>,
}

#[derive(Serialize)]
pub(super) struct ChatMessage {
    pub(super) role: &'static str,
    pub(super) content: String,
}

#[derive(Serialize)]
pub(super) struct ChatResponseFormat {
    #[serde(rename = "type")]
    pub(super) format_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) json_schema: Option<ChatResponseFormatJsonSchema>,
}

#[derive(Serialize)]
pub(super) struct ChatResponseFormatJsonSchema {
    pub(super) name: &'static str,
    pub(super) strict: bool,
    pub(super) schema: Value,
}

pub(crate) fn build_prompt(repo_ctx: &RepoContext) -> String {
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

pub(crate) fn models_url(base: &str) -> String {
    api_endpoint_url(base, "models")
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
    use super::{api_endpoint_url, build_prompt, build_prompt_scaffold, models_url};
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
        assert_eq!(
            models_url("http://localhost:11434"),
            "http://localhost:11434/v1/models"
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
