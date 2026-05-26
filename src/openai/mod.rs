mod context;
mod request;
mod response;
mod stream;

use crate::config::Config;
use crate::message::validate_message;
use crate::provider_common::provider_debug_enabled;
use async_openai::Client;
use async_openai::config::Config as AsyncOpenAiConfig;
use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    CreateChatCompletionRequest, ChatCompletionResponseStream,
};
use async_openai::types::responses::{CreateResponse, CreateResponseArgs, InputParam, ResponseStream};
use futures::StreamExt;
use reqwest::RequestBuilder;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use std::time::{Duration, Instant};

pub(crate) use self::request::models_url;
pub(crate) use self::request::{MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, build_prompt, build_prompt_scaffold};
pub(crate) use self::stream::StreamRenderer;
pub(crate) use context::{detect_model_context_tokens, resolve_model_context_config};
use request::ApiEndpointPreference;
use response::{
    ResponseTextAccumulator, append_response_stream_event_text, extract_chat_message,
    extract_response_text, should_fallback_from_responses_message,
    should_retry_without_stream_message,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct GenerationMetrics {
    pub api_duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamOutput {
    None,
    Stdout,
}

#[derive(Clone, Debug)]
pub(crate) struct OpenAiCompatibleConfig {
    api_base: String,
    api_key: SecretString,
    send_bearer_auth: bool,
}

impl OpenAiCompatibleConfig {
    fn from_app_config(cfg: &Config) -> Self {
        Self {
            api_base: cfg.api_base.clone(),
            api_key: SecretString::from(cfg.api_key.clone()),
            send_bearer_auth: cfg.should_send_bearer_auth(),
        }
    }
}

impl AsyncOpenAiConfig for OpenAiCompatibleConfig {
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if self.send_bearer_auth {
            let value = format!("Bearer {}", self.api_key.expose_secret());
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&value).expect("valid authorization header"),
            );
        }
        headers
    }

    fn url(&self, path: &str) -> String {
        let endpoint = path.trim_start_matches('/');
        request::api_endpoint_url(&self.api_base, endpoint)
    }

    fn query(&self) -> Vec<(&str, &str)> {
        Vec::new()
    }

    fn api_base(&self) -> &str {
        &self.api_base
    }

    fn api_key(&self) -> &SecretString {
        &self.api_key
    }
}

pub async fn generate_message_with_stream_output(
    cfg: &Config,
    repo_ctx: &crate::git::RepoContext,
    stream_output: StreamOutput,
    debug_provider: bool,
) -> Result<(String, GenerationMetrics), String> {
    if cfg.should_use_anthropic_transport() {
        return crate::anthropic::generate_anthropic_message_with_stream_output(
            cfg,
            repo_ctx,
            stream_output,
            debug_provider,
        )
        .await;
    }
    generate_openai_message_with_stream_output(cfg, repo_ctx, stream_output, debug_provider).await
}

pub(crate) async fn generate_openai_message_with_stream_output(
    cfg: &Config,
    repo_ctx: &crate::git::RepoContext,
    stream_output: StreamOutput,
    debug_provider: bool,
) -> Result<(String, GenerationMetrics), String> {
    let prompt = build_prompt(repo_ctx);
    let started = Instant::now();
    let mut renderer = StreamRenderer::new(stream_output);
    let debug_enabled = provider_debug_enabled(debug_provider);

    let message = match request::endpoint_preference(&cfg.api_base) {
        ApiEndpointPreference::ResponsesOnly => {
            generate_message_via_responses(cfg, &prompt, &mut renderer, debug_provider)
                .await
                .map_err(|err| err.message)?
        }
        ApiEndpointPreference::ChatCompletionsOnly => {
            generate_message_via_chat_completions(cfg, &prompt, &mut renderer, debug_provider)
                .await?
        }
        ApiEndpointPreference::Auto => match generate_message_via_responses(
            cfg,
            &prompt,
            &mut renderer,
            debug_provider,
        )
        .await
        {
            Ok(message) => message,
            Err(err) if err.should_fallback => {
                if debug_enabled {
                    eprintln!(
                        "git-ai-commit: provider debug: responses failed, falling back to chat/completions: {}",
                        err.message
                    );
                }
                renderer.reset();
                generate_message_via_chat_completions(
                    cfg,
                    &prompt,
                    &mut renderer,
                    debug_provider,
                )
                .await?
            }
            Err(err) => return Err(err.message),
        },
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

pub(crate) fn new_openai_client(cfg: &Config) -> Result<Client<OpenAiCompatibleConfig>, String> {
    let http_client = crate::provider_common::new_http_client(cfg)?;
    Ok(Client::with_config(OpenAiCompatibleConfig::from_app_config(cfg)).with_http_client(http_client))
}

async fn generate_message_via_responses(
    cfg: &Config,
    prompt: &str,
    renderer: &mut StreamRenderer,
    debug_provider: bool,
) -> Result<String, ApiAttemptError> {
    let debug_enabled = provider_debug_enabled(debug_provider);
    let request = build_responses_request(cfg, prompt)?;

    if renderer.enabled() {
        match run_responses_stream_once(cfg, request.clone(), renderer, debug_enabled).await {
            Ok(message) => Ok(message),
            Err(err) if should_retry_without_stream_message(&err.message) => {
                if debug_enabled {
                    eprintln!(
                        "git-ai-commit: provider debug: responses stream failed, retrying without stream: {}",
                        err.message
                    );
                }
                match run_responses_non_stream_once(cfg, request, debug_enabled).await {
                    Ok(message) => Ok(message),
                    Err(err) => Err(ApiAttemptError {
                        message: err.message,
                        should_fallback: true,
                    }),
                }
            }
            Err(err) => Err(err),
        }
    } else {
        run_responses_non_stream_once(cfg, request, debug_enabled)
            .await
            .map_err(|err| ApiAttemptError {
                message: err.message,
                should_fallback: true,
            })
    }
}

async fn generate_message_via_chat_completions(
    cfg: &Config,
    prompt: &str,
    renderer: &mut StreamRenderer,
    debug_provider: bool,
) -> Result<String, String> {
    let debug_enabled = provider_debug_enabled(debug_provider);
    let request = build_chat_request(cfg, prompt)?;

    if renderer.enabled() {
        match run_chat_stream_once(cfg, request.clone(), renderer, debug_enabled).await {
            Ok(message) => Ok(message),
            Err(err) if should_retry_without_stream_message(&err) => {
                if debug_enabled {
                    eprintln!(
                        "git-ai-commit: provider debug: chat.completions stream failed, retrying without stream: {}",
                        err
                    );
                }
                run_chat_non_stream_once(cfg, request, debug_enabled).await
            }
            Err(err) => Err(err),
        }
    } else {
        run_chat_non_stream_once(cfg, request, debug_enabled).await
    }
}

fn build_responses_request(cfg: &Config, prompt: &str) -> Result<CreateResponse, ApiAttemptError> {
    CreateResponseArgs::default()
        .model(&cfg.model)
        .instructions(SYSTEM_PROMPT)
        .input(InputParam::Text(prompt.to_string()))
        .max_output_tokens(MAX_OUTPUT_TOKENS as u32)
        .build()
        .map_err(|err| ApiAttemptError {
            message: err.to_string(),
            should_fallback: false,
        })
}

fn build_chat_request(cfg: &Config, prompt: &str) -> Result<CreateChatCompletionRequest, String> {
    let system_message = ChatCompletionRequestSystemMessageArgs::default()
        .content(SYSTEM_PROMPT)
        .build()
        .map(ChatCompletionRequestMessage::System)
        .map_err(|err| err.to_string())?;
    let user_message = ChatCompletionRequestUserMessageArgs::default()
        .content(prompt)
        .build()
        .map(ChatCompletionRequestMessage::User)
        .map_err(|err| err.to_string())?;

    CreateChatCompletionRequestArgs::default()
        .model(&cfg.model)
        .messages(vec![system_message, user_message])
        .max_completion_tokens(MAX_OUTPUT_TOKENS as u32)
        .build()
        .map_err(|err| err.to_string())
}

async fn run_responses_stream_once(
    cfg: &Config,
    request: CreateResponse,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=true",
            request::responses_url(&cfg.api_base),
            cfg.model,
        );
    }

    let client = new_openai_client(cfg).map_err(|message| ApiAttemptError {
        message,
        should_fallback: false,
    })?;
    let stream = client
        .responses()
        .create_stream(request)
        .await
        .map_err(openai_error_to_attempt_error)?;

    collect_response_stream(stream, renderer, debug_enabled).await
}

async fn run_responses_non_stream_once(
    cfg: &Config,
    request: CreateResponse,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=false",
            request::responses_url(&cfg.api_base),
            cfg.model,
        );
    }

    let client = new_openai_client(cfg).map_err(|message| ApiAttemptError {
        message,
        should_fallback: false,
    })?;
    let response = client
        .responses()
        .create(request)
        .await
        .map_err(openai_error_to_attempt_error)?;

    extract_response_text(response, debug_enabled).map_err(|message| ApiAttemptError {
        should_fallback: response::should_fallback_from_empty_responses_payload(&message),
        message,
    })
}

async fn run_chat_stream_once(
    cfg: &Config,
    request: CreateChatCompletionRequest,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, String> {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=true",
            request::chat_completions_url(&cfg.api_base),
            cfg.model,
        );
    }

    let client = new_openai_client(cfg)?;
    let stream: ChatCompletionResponseStream = client
        .chat()
        .create_stream(request)
        .await
        .map_err(|err| err.to_string())?;

    response::collect_chat_completion_stream(stream, renderer).await
}

async fn run_chat_non_stream_once(
    cfg: &Config,
    request: CreateChatCompletionRequest,
    debug_enabled: bool,
) -> Result<String, String> {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=false",
            request::chat_completions_url(&cfg.api_base),
            cfg.model,
        );
    }

    let client = new_openai_client(cfg)?;
    let response = client
        .chat()
        .create(request)
        .await
        .map_err(|err| err.to_string())?;

    extract_chat_message(response, debug_enabled)
}

async fn collect_response_stream(
    mut stream: ResponseStream,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    let mut accumulator = ResponseTextAccumulator::default();
    let mut error_message = None;

    while let Some(event) = stream.next().await {
        if let Some(message) = append_response_stream_event_text(
            event.map_err(openai_error_to_attempt_error)?,
            renderer,
            &mut accumulator,
            debug_enabled,
        )
        .map_err(|message| ApiAttemptError {
            message,
            should_fallback: false,
        })? {
            error_message = Some(message);
        }
    }

    if !accumulator.content().trim().is_empty() {
        return Ok(crate::message::sanitize_message(accumulator.content()));
    }

    let message = error_message
        .unwrap_or_else(|| "responses request returned no output text".to_string());
    Err(ApiAttemptError {
        should_fallback: should_fallback_from_responses_message(&message)
            || response::should_fallback_from_empty_responses_payload(&message),
        message,
    })
}

fn openai_error_to_attempt_error(err: async_openai::error::OpenAIError) -> ApiAttemptError {
    match err {
        async_openai::error::OpenAIError::ApiError(error) => {
            let message = error.to_string();
            ApiAttemptError {
                should_fallback: response::should_fallback_from_responses(
                    error.status_code.as_u16(),
                    &error.api_error.to_string(),
                ),
                message,
            }
        }
        other => {
            let message = other.to_string();
            ApiAttemptError {
                should_fallback: should_fallback_from_responses_message(&message),
                message,
            }
        }
    }
}
