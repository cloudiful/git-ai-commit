mod context;
mod request;
mod response;
mod stream;
mod structured;

use crate::config::Config;
use crate::message::validate_message;
use crate::provider_transport::{AnthropicTransport, CommitMessageTransport, OpenAiTransport};
use crate::provider_common::{new_http_client, provider_debug_enabled};
use reqwest::blocking::Client;
use reqwest::blocking::RequestBuilder;
use std::time::{Duration, Instant};

use self::request::{
    ChatCompletionRequest, ChatMessage, ResponseInputMessage, ResponsesRequest, chat_completions_url,
    responses_url,
};
use self::response::{
    parse_chat_completion_response, parse_responses_response, should_fallback_from_responses,
};
pub(crate) use self::request::models_url;
pub(crate) use self::request::{
    MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, build_prompt, build_prompt_scaffold,
};
pub(crate) use self::stream::StreamRenderer;
pub(crate) use context::{detect_model_context_tokens, resolve_model_context_config};

#[derive(Clone, Copy, Debug, Default)]
pub struct GenerationMetrics {
    pub api_duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamOutput {
    None,
    Stdout,
    Stderr,
}

impl CommitMessageTransport for OpenAiTransport {
    fn generate(
        &self,
        cfg: &Config,
        repo_ctx: &crate::git::RepoContext,
        stream_output: StreamOutput,
        debug_provider: bool,
    ) -> Result<(String, GenerationMetrics), String> {
        generate_openai_message_with_stream_output(cfg, repo_ctx, stream_output, debug_provider)
    }
}

pub fn generate_message_with_stream_output(
    cfg: &Config,
    repo_ctx: &crate::git::RepoContext,
    stream_output: StreamOutput,
    debug_provider: bool,
) -> Result<(String, GenerationMetrics), String> {
    if cfg.should_use_anthropic_transport() {
        return AnthropicTransport.generate(cfg, repo_ctx, stream_output, debug_provider);
    }
    OpenAiTransport.generate(cfg, repo_ctx, stream_output, debug_provider)
}

pub(crate) fn generate_openai_message_with_stream_output(
    cfg: &Config,
    repo_ctx: &crate::git::RepoContext,
    stream_output: StreamOutput,
    debug_provider: bool,
) -> Result<(String, GenerationMetrics), String> {
    let client = new_http_client(cfg)?;
    let prompt = build_prompt(repo_ctx);
    let started = Instant::now();
    let mut renderer = StreamRenderer::new(stream_output);
    let debug_enabled = provider_debug_enabled(debug_provider);
    match structured::generate_structured_message_via_chat_completions(cfg, &client, &prompt, debug_provider) {
        Ok(message) => {
            if debug_enabled {
                eprintln!("git-ai-commit: provider debug: structured output accepted");
            }
            renderer.push(&message).map_err(|err| err.to_string())?;
            renderer.finish().map_err(|err| err.to_string())?;
            let metrics = GenerationMetrics {
                api_duration: started.elapsed(),
            };
            validate_message(&message)?;
            return Ok((message, metrics));
        }
        Err(err) => {
            if debug_enabled {
                eprintln!(
                    "git-ai-commit: provider debug: structured output failed, falling back to plain chat: {}",
                    err
                );
            }
        }
    }

    match generate_message_via_chat_completions(cfg, &client, &prompt, &mut renderer, debug_provider)
    {
        Ok(message) => {
            renderer.finish().map_err(|err| err.to_string())?;
            let metrics = GenerationMetrics {
                api_duration: started.elapsed(),
            };
            validate_message(&message)?;
            return Ok((message, metrics));
        }
        Err(err) => {
            if debug_enabled {
                eprintln!(
                    "git-ai-commit: provider debug: plain chat failed, falling back to responses: {}",
                    err
                );
            }
            renderer.reset();
        }
    }

    let message = match generate_message_via_responses(
        cfg,
        &client,
        &prompt,
        &mut renderer,
        debug_provider,
    ) {
        Ok(message) => message,
        Err(err) if err.should_fallback => {
            renderer.reset();
            return Err(err.message);
        }
        Err(err) => return Err(err.message),
    };
    renderer.finish().map_err(|err| err.to_string())?;
    let metrics = GenerationMetrics {
        api_duration: started.elapsed(),
    };
    validate_message(&message)?;
    Ok((message, metrics))
}

#[derive(Debug)]
struct ApiAttemptError {
    message: String,
    should_fallback: bool,
}

pub(crate) fn apply_auth(builder: RequestBuilder, cfg: &Config) -> RequestBuilder {
    if cfg.should_send_bearer_auth() {
        builder.bearer_auth(&cfg.api_key)
    } else {
        builder
    }
}

fn generate_message_via_responses(
    cfg: &Config,
    client: &Client,
    prompt: &str,
    renderer: &mut StreamRenderer,
    debug_provider: bool,
) -> Result<String, ApiAttemptError> {
    let debug_enabled = provider_debug_enabled(debug_provider);
    let request = ResponsesRequest {
        model: cfg.model.clone(),
        instructions: SYSTEM_PROMPT.to_string(),
        input: vec![ResponseInputMessage {
            role: "user",
            content: prompt.to_string(),
        }],
        temperature: 0.2,
        max_output_tokens: MAX_OUTPUT_TOKENS as u32,
        stream: renderer.enabled(),
    };

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream={}",
            responses_url(&cfg.api_base),
            cfg.model,
            renderer.enabled()
        );
    }

    let response = apply_auth(client.post(responses_url(&cfg.api_base)), cfg)
        .json(&request)
        .send()
        .map_err(|err| ApiAttemptError {
            message: err.to_string(),
            should_fallback: false,
        })?;

    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();

    parse_responses_response(
        status_code,
        &content_type,
        response,
        renderer,
        debug_enabled,
    )
    .map_err(|message| {
        let should_fallback = should_fallback_from_responses(status_code, &message);
        ApiAttemptError {
            message,
            should_fallback,
        }
    })
}

fn generate_message_via_chat_completions(
    cfg: &Config,
    client: &Client,
    prompt: &str,
    renderer: &mut StreamRenderer,
    debug_provider: bool,
) -> Result<String, String> {
    let debug_enabled = provider_debug_enabled(debug_provider);
    let request = ChatCompletionRequest {
        model: cfg.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system",
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user",
                content: prompt.to_string(),
            },
        ],
        temperature: 0.2,
        max_tokens: MAX_OUTPUT_TOKENS as u32,
        stream: renderer.enabled(),
        response_format: None,
    };

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream={}",
            chat_completions_url(&cfg.api_base),
            cfg.model,
            renderer.enabled()
        );
    }

    let response = apply_auth(client.post(chat_completions_url(&cfg.api_base)), cfg)
        .json(&request)
        .send()
        .map_err(|err| err.to_string())?;

    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();

    parse_chat_completion_response(
        status_code,
        &content_type,
        response,
        renderer,
        debug_enabled,
    )
}
