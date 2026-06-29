mod byot;
mod context;
mod request;
mod response;
mod stream;

use crate::config::Config;
use crate::message::validate_message;
use crate::provider_common::provider_debug_enabled;
use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequest,
    CreateChatCompletionRequestArgs,
};
use async_openai::types::responses::{CreateResponse, CreateResponseArgs, InputParam};
use reqwest::RequestBuilder;
use std::time::{Duration, Instant};

pub(crate) use self::request::models_url;
pub(crate) use self::request::{
    MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, build_prompt, build_prompt_scaffold,
};
pub(crate) use self::stream::StreamRenderer;
pub(crate) use context::{detect_model_context_tokens, resolve_model_context_config};
use request::ApiEndpointPreference;
use response::should_retry_without_stream_message;

#[derive(Clone, Copy, Debug, Default)]
pub struct GenerationMetrics {
    pub api_duration: Duration,
    pub streamed_render_completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamOutput {
    None,
    Stdout,
}

#[derive(Debug)]
struct ApiAttemptError {
    message: String,
    should_fallback: bool,
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
        ApiEndpointPreference::Auto => {
            match generate_message_via_responses(cfg, &prompt, &mut renderer, debug_provider).await
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
            }
        }
    };

    renderer.finish().map_err(|err| err.to_string())?;
    let metrics = GenerationMetrics {
        api_duration: started.elapsed(),
        streamed_render_completed: renderer.completed_render(),
    };
    validate_message(&message)?;
    Ok((message, metrics))
}

pub(crate) fn apply_auth(builder: RequestBuilder, cfg: &Config) -> RequestBuilder {
    if cfg.should_send_bearer_auth() {
        builder.bearer_auth(&cfg.api_key)
    } else {
        builder
    }
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
        match byot::run_responses_stream_once(cfg, &request, renderer, debug_enabled).await {
            Ok(message) => Ok(message),
            Err(err) if should_retry_without_stream_message(&err.message) => {
                if debug_enabled {
                    eprintln!(
                        "git-ai-commit: provider debug: responses stream failed, retrying without stream: {}",
                        err.message
                    );
                    byot::diagnose_raw_responses_stream(cfg, &request).await;
                }
                match byot::run_responses_non_stream_once(cfg, &request, debug_enabled).await {
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
        byot::run_responses_non_stream_once(cfg, &request, debug_enabled)
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
        match byot::run_chat_stream_once(cfg, &request, renderer, debug_enabled).await {
            Ok(message) => Ok(message),
            Err(err) if should_retry_without_stream_message(&err) => {
                if debug_enabled {
                    eprintln!(
                        "git-ai-commit: provider debug: chat.completions stream failed, retrying without stream: {}",
                        err
                    );
                }
                byot::run_chat_non_stream_once(cfg, &request, debug_enabled).await
            }
            Err(err) => Err(err),
        }
    } else {
        byot::run_chat_non_stream_once(cfg, &request, debug_enabled).await
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
