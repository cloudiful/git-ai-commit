use crate::git::RepoContext;
use reqwest::Url;

pub(crate) const SYSTEM_PROMPT: &str = "You are an expert at writing Git commit messages. Your job is to write a short, clear commit message that summarizes the staged changes.\n\nUse English Conventional Commit style for the subject line.\n\nIf you can accurately express the change in just the subject line, do not include a message body. Only use the body when it provides useful information. Do not repeat information from the subject line in the body.\n\nReturn only the final commit message. Do not explain your reasoning. Do not describe the task. Do not preface the answer. Do not include code fences. Do not include the raw diff in the commit message.\n\nFollow good Git style:\n- Separate the subject from the body with a blank line\n- Keep the subject line within 72 characters\n- Use the imperative mood in the subject line\n- Keep the body short and concise\n- Do not invent behavior not present in the diff";
pub(crate) const MAX_OUTPUT_TOKENS: usize = 4096;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApiEndpointPreference {
    Auto,
    ResponsesOnly,
    ChatCompletionsOnly,
}

pub(crate) fn endpoint_preference(base: &str) -> ApiEndpointPreference {
    let url = Url::parse(base.trim()).unwrap_or_else(|_| {
        panic!("invalid ai.commit.apiBase URL {:?}", base);
    });
    match normalized_base_segments(&url).explicit_endpoint {
        ExplicitEndpoint::Responses => ApiEndpointPreference::ResponsesOnly,
        ExplicitEndpoint::ChatCompletions => ApiEndpointPreference::ChatCompletionsOnly,
        ExplicitEndpoint::Models | ExplicitEndpoint::None => ApiEndpointPreference::Auto,
    }
}

pub(crate) fn api_endpoint_url(base: &str, endpoint: &str) -> String {
    let mut url = Url::parse(base.trim()).unwrap_or_else(|_| {
        panic!("invalid ai.commit.apiBase URL {:?}", base);
    });
    let normalized = normalized_base_segments(&url);
    let mut segments = normalized.segments;
    if !normalized.had_known_endpoint && !segments.last().is_some_and(|segment| *segment == "v1") {
        segments.push("v1");
    }
    segments.extend(endpoint.split('/'));
    url.set_path(&format!("/{}", segments.join("/")));
    url.set_query(None);
    url.to_string()
}

struct NormalizedBaseSegments<'a> {
    segments: Vec<&'a str>,
    had_known_endpoint: bool,
    explicit_endpoint: ExplicitEndpoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExplicitEndpoint {
    None,
    Responses,
    ChatCompletions,
    Models,
}

fn normalized_base_segments(url: &Url) -> NormalizedBaseSegments<'_> {
    let mut segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let explicit_endpoint = match segments.as_slice() {
        [.., "chat", "completions"] => {
            segments.truncate(segments.len() - 2);
            ExplicitEndpoint::ChatCompletions
        }
        [.., "responses"] => {
            segments.truncate(segments.len() - 1);
            ExplicitEndpoint::Responses
        }
        [.., "models"] => {
            segments.truncate(segments.len() - 1);
            ExplicitEndpoint::Models
        }
        _ => ExplicitEndpoint::None,
    };
    let had_known_endpoint = !matches!(explicit_endpoint, ExplicitEndpoint::None);

    NormalizedBaseSegments {
        segments,
        had_known_endpoint,
        explicit_endpoint,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApiEndpointPreference, api_endpoint_url, build_prompt, build_prompt_scaffold,
        endpoint_preference, models_url,
    };
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
        assert_eq!(
            api_endpoint_url("https://example.com/openai/v1/models", "responses"),
            "https://example.com/openai/v1/responses"
        );
        assert_eq!(
            api_endpoint_url("https://example.com/openai", "chat/completions"),
            "https://example.com/openai/v1/chat/completions"
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

    #[test]
    fn detects_explicit_endpoint_preferences() {
        assert_eq!(
            endpoint_preference("https://api.openai.com/v1/chat/completions"),
            ApiEndpointPreference::ChatCompletionsOnly
        );
        assert_eq!(
            endpoint_preference("https://api.openai.com/v1/responses"),
            ApiEndpointPreference::ResponsesOnly
        );
        assert_eq!(
            endpoint_preference("https://api.openai.com/v1"),
            ApiEndpointPreference::Auto
        );
        assert_eq!(
            endpoint_preference("https://example.com/openai/v1/models"),
            ApiEndpointPreference::Auto
        );
    }
}
