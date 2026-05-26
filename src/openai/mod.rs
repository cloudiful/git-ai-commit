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
};
use async_openai::types::responses::{CreateResponseArgs, InputParam, ResponseStream};
use futures::StreamExt;
use reqwest::RequestBuilder;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use std::time::{Duration, Instant};

pub(crate) use self::request::models_url;
pub(crate) use self::request::{MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, build_prompt, build_prompt_scaffold};
pub(crate) use self::stream::StreamRenderer;
pub(crate) use context::{detect_model_context_tokens, resolve_model_context_config};
use response::{
    append_response_stream_event_text, extract_chat_message, extract_response_text,
    should_fallback_from_responses_message,
};

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
    let client = new_openai_client(cfg)?;
    let prompt = build_prompt(repo_ctx);
    let started = Instant::now();
    let mut renderer = StreamRenderer::new(stream_output);
    let debug_enabled = provider_debug_enabled(debug_provider);

    let message = match generate_message_via_responses(
        cfg,
        &client,
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
                    "git-ai-commit: provider debug: responses unsupported, falling back to chat/completions: {}",
                    err.message
                );
            }
            renderer.reset();
            generate_message_via_chat_completions(
                cfg,
                &client,
                &prompt,
                &mut renderer,
                debug_provider,
            )
            .await?
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

pub(crate) fn new_openai_client(cfg: &Config) -> Result<Client<OpenAiCompatibleConfig>, String> {
    let http_client = crate::provider_common::new_http_client(cfg)?;
    Ok(Client::with_config(OpenAiCompatibleConfig::from_app_config(cfg)).with_http_client(http_client))
}

async fn generate_message_via_responses(
    cfg: &Config,
    client: &Client<OpenAiCompatibleConfig>,
    prompt: &str,
    renderer: &mut StreamRenderer,
    debug_provider: bool,
) -> Result<String, ApiAttemptError> {
    let debug_enabled = provider_debug_enabled(debug_provider);
    let request = CreateResponseArgs::default()
        .model(&cfg.model)
        .instructions(SYSTEM_PROMPT)
        .input(InputParam::Text(prompt.to_string()))
        .max_output_tokens(MAX_OUTPUT_TOKENS as u32)
        .build()
        .map_err(|err| ApiAttemptError {
            message: err.to_string(),
            should_fallback: false,
        })?;

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream={}",
            request::responses_url(&cfg.api_base),
            cfg.model,
            renderer.enabled()
        );
    }

    if renderer.enabled() {
        let stream = client
            .responses()
            .create_stream(request)
            .await
            .map_err(openai_error_to_attempt_error)?;
        collect_response_stream(stream, renderer, debug_enabled).await
    } else {
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
}

async fn generate_message_via_chat_completions(
    cfg: &Config,
    client: &Client<OpenAiCompatibleConfig>,
    prompt: &str,
    renderer: &mut StreamRenderer,
    debug_provider: bool,
) -> Result<String, String> {
    let debug_enabled = provider_debug_enabled(debug_provider);
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
    let request = CreateChatCompletionRequestArgs::default()
        .model(&cfg.model)
        .messages(vec![system_message, user_message])
        .max_completion_tokens(MAX_OUTPUT_TOKENS as u32)
        .build()
        .map_err(|err| err.to_string())?;

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream={}",
            request::chat_completions_url(&cfg.api_base),
            cfg.model,
            renderer.enabled()
        );
    }

    if renderer.enabled() {
        let stream = client
            .chat()
            .create_stream(request)
            .await
            .map_err(|err| err.to_string())?;
        response::collect_chat_completion_stream(stream, renderer).await
    } else {
        let response = client
            .chat()
            .create(request)
            .await
            .map_err(|err| err.to_string())?;
        extract_chat_message(response, debug_enabled)
    }
}

async fn collect_response_stream(
    mut stream: ResponseStream,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    let mut content = String::new();
    let mut error_message = None;

    while let Some(event) = stream.next().await {
        if let Some(message) = append_response_stream_event_text(
            event.map_err(openai_error_to_attempt_error)?,
            renderer,
            &mut content,
            debug_enabled,
        )
        .map_err(|message| ApiAttemptError {
            message,
            should_fallback: false,
        })? {
            error_message = Some(message);
        }
    }

    if !content.trim().is_empty() {
        return Ok(crate::message::sanitize_message(&content));
    }

    Err(ApiAttemptError {
        should_fallback: response::should_fallback_from_empty_responses_payload(
            error_message
                .as_deref()
                .unwrap_or("responses request returned no output text"),
        ),
        message: error_message.unwrap_or_else(|| "responses request returned no output text".to_string()),
    })
}

fn openai_error_to_attempt_error(err: async_openai::error::OpenAIError) -> ApiAttemptError {
    let message = err.to_string();
    ApiAttemptError {
        should_fallback: should_fallback_from_responses_message(&message),
        message,
    }
}
