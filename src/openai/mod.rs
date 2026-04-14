mod request;
mod response;
mod stream;

use crate::config::Config;
use crate::message::validate_message;
use reqwest::blocking::Client;
use std::time::{Duration, Instant};

use self::request::{
    ChatCompletionRequest, ChatMessage, ResponseInputMessage, ResponsesRequest, build_prompt,
    chat_completions_url, responses_url,
};
use self::response::{
    parse_chat_completion_response, parse_responses_response, should_fallback_from_responses,
};
use self::stream::StreamRenderer;

pub(crate) use self::request::{MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, build_prompt_scaffold};

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

pub fn generate_message_with_stream_output(
    cfg: &Config,
    repo_ctx: &crate::git::RepoContext,
    stream_output: StreamOutput,
) -> Result<(String, GenerationMetrics), String> {
    let client = new_http_client(cfg)?;
    let prompt = build_prompt(repo_ctx);
    let started = Instant::now();
    let mut renderer = StreamRenderer::new(stream_output);
    let message = match generate_message_via_responses(cfg, &client, &prompt, &mut renderer) {
        Ok(message) => message,
        Err(err) if err.should_fallback => {
            renderer.reset();
            generate_message_via_chat_completions(cfg, &client, &prompt, &mut renderer).map_err(
                |fallback_err| format!("{fallback_err} (responses fallback: {})", err.message),
            )?
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

fn new_http_client(cfg: &Config) -> Result<Client, String> {
    let mut builder = Client::builder().timeout(cfg.timeout);
    if !cfg.use_env_proxy {
        builder = builder.no_proxy();
    }
    builder.build().map_err(|err| err.to_string())
}

fn generate_message_via_responses(
    cfg: &Config,
    client: &Client,
    prompt: &str,
    renderer: &mut StreamRenderer,
) -> Result<String, ApiAttemptError> {
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

    let response = client
        .post(responses_url(&cfg.api_base))
        .bearer_auth(&cfg.api_key)
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

    parse_responses_response(status_code, &content_type, response, renderer).map_err(|message| {
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
) -> Result<String, String> {
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
    };

    let response = client
        .post(chat_completions_url(&cfg.api_base))
        .bearer_auth(&cfg.api_key)
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

    parse_chat_completion_response(status_code, &content_type, response, renderer)
}
