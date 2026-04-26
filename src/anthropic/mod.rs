use crate::config::Config;
use crate::message::{sanitize_message, validate_message};
use crate::openai::{GenerationMetrics, StreamOutput};
use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::time::Instant;

const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    system: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f64,
    stream: bool,
}

#[derive(Serialize)]
struct Message {
    role: &'static str,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    error: Option<ProviderError>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type", default)]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct ProviderError {
    message: String,
}

pub fn generate_message(
    cfg: &Config,
    repo_ctx: &crate::git::RepoContext,
    stream_output: StreamOutput,
    debug_provider: bool,
) -> Result<(String, GenerationMetrics), String> {
    let debug_enabled = debug_provider
        || matches!(
            std::env::var("GIT_AI_COMMIT_DEBUG_PROVIDER")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("1" | "true" | "yes" | "on")
        );
    let client = Client::builder()
        .timeout(cfg.timeout)
        .build()
        .map_err(|err| err.to_string())?;
    let prompt = crate::openai::build_prompt(repo_ctx);

    let started = Instant::now();
    let request = MessagesRequest {
        model: cfg.model.clone(),
        system: crate::openai::SYSTEM_PROMPT.to_string(),
        messages: vec![Message {
            role: "user",
            content: prompt,
        }],
        max_tokens: crate::openai::MAX_OUTPUT_TOKENS as u32,
        temperature: 0.2,
        stream: false,
    };
    let url = messages_url(&cfg.api_base);
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=false anthropic_version={}",
            url, cfg.model, ANTHROPIC_VERSION
        );
    }
    let response = apply_auth(client.post(&url), cfg)
        .json(&request)
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status().as_u16();
    let body = response.text().map_err(|err| err.to_string())?;
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: anthropic status={} body={}",
            status,
            truncate_debug_body(&body)
        );
    }
    let parsed: MessagesResponse = serde_json::from_str(&body).map_err(|err| {
        if status >= 400 {
            format!("anthropic messages request failed: {}", body.trim())
        } else {
            format!("invalid anthropic messages payload: {err}")
        }
    })?;
    if status >= 400 {
        return Err(parsed
            .error
            .map(|e| e.message)
            .unwrap_or_else(|| format!("anthropic messages request failed with status {status}")));
    }
    let text = parsed
        .content
        .into_iter()
        .filter(|part| part.block_type == "text")
        .filter_map(|part| part.text)
        .collect::<String>();
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: anthropic extracted text={}",
            truncate_debug_body(&text)
        );
    }
    let message = sanitize_message(&text);
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: anthropic sanitized message={}",
            truncate_debug_body(&message)
        );
    }
    validate_message(&message)?;
    if matches!(stream_output, StreamOutput::Stdout) {
        println!("{message}");
    } else if matches!(stream_output, StreamOutput::Stderr) {
        eprintln!("\n{message}");
    }
    Ok((
        message,
        GenerationMetrics {
            api_duration: started.elapsed(),
        },
    ))
}

fn messages_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else if base.ends_with("/messages") {
        base.to_string()
    } else {
        format!("{base}/v1/messages")
    }
}

fn apply_auth(builder: RequestBuilder, cfg: &Config) -> RequestBuilder {
    builder
        .header("x-api-key", cfg.api_key.as_str())
        .header("anthropic-version", ANTHROPIC_VERSION)
}

fn truncate_debug_body(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(400).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}
