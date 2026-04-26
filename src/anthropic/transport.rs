use crate::config::Config;
use crate::message::{sanitize_message, validate_message};
use crate::openai::{GenerationMetrics, StreamOutput, StreamRenderer};
use crate::provider_common::{new_http_client, provider_debug_enabled, truncate_debug_body};
use crate::provider_transport::{AnthropicTransport, CommitMessageTransport};
use reqwest::blocking::RequestBuilder;
use std::time::Instant;

use super::request::{
    ANTHROPIC_VERSION, Message, MessagesRequest, disabled_thinking, messages_url,
};
use super::response::MessagesResponse;

pub(crate) fn generate_anthropic_message_with_stream_output(
    cfg: &Config,
    repo_ctx: &crate::git::RepoContext,
    stream_output: StreamOutput,
    debug_provider: bool,
) -> Result<(String, GenerationMetrics), String> {
    let debug_enabled = provider_debug_enabled(debug_provider);
    let client = new_http_client(cfg)?;
    let prompt = crate::openai::build_prompt(repo_ctx);
    let mut renderer = StreamRenderer::new(stream_output);

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
        thinking: disabled_thinking(&cfg.api_base),
    };
    let url = messages_url(&cfg.api_base);
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=false anthropic_version={} thinking={}",
            url,
            cfg.model,
            ANTHROPIC_VERSION,
            request.thinking.as_ref().map(|cfg| cfg.kind).unwrap_or("default")
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
    let block_types = parsed.block_types();
    let has_thinking = parsed.has_thinking();
    let text = parsed.text_content();
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: anthropic extracted text={}",
            truncate_debug_body(&text)
        );
        eprintln!(
            "git-ai-commit: provider debug: anthropic content block types={:?}",
            block_types
        );
    }
    if text.trim().is_empty() {
        if has_thinking {
            return Err(
                "anthropic response contained only thinking blocks and no final text; try disabling provider thinking or using the OpenAI-compatible endpoint"
                    .to_string(),
            );
        }
        return Err("anthropic response did not contain any text blocks".to_string());
    }
    let message = sanitize_message(&text);
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: anthropic sanitized message={}",
            truncate_debug_body(&message)
        );
    }
    validate_message(&message)?;
    renderer.push(&message).map_err(|err| err.to_string())?;
    renderer.finish().map_err(|err| err.to_string())?;
    Ok((
        message,
        GenerationMetrics {
            api_duration: started.elapsed(),
        },
    ))
}

impl CommitMessageTransport for AnthropicTransport {
    fn generate(
        &self,
        cfg: &Config,
        repo_ctx: &crate::git::RepoContext,
        stream_output: StreamOutput,
        debug_provider: bool,
    ) -> Result<(String, GenerationMetrics), String> {
        generate_anthropic_message_with_stream_output(cfg, repo_ctx, stream_output, debug_provider)
    }
}

fn apply_auth(builder: RequestBuilder, cfg: &Config) -> RequestBuilder {
    builder
        .header("x-api-key", cfg.api_key.as_str())
        .header("anthropic-version", ANTHROPIC_VERSION)
}
