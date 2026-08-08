use super::{
    ApiAttemptError, apply_auth,
    provider_defaults::{EndpointKind, apply_provider_body_defaults},
    request, response, sse,
};
use crate::config::Config;
use crate::provider_common::truncate_debug_body;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_TYPE};
use reqwest::{RequestBuilder, Response};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TransportEndpoint {
    Responses,
    ChatCompletions,
}

impl TransportEndpoint {
    pub(super) fn endpoint_kind(self) -> EndpointKind {
        match self {
            Self::Responses => EndpointKind::Responses,
            Self::ChatCompletions => EndpointKind::ChatCompletions,
        }
    }

    pub(super) fn request_label(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat.completions",
        }
    }

    pub(super) fn stream_event_label(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat",
        }
    }

    pub(super) fn stream_debug_label(self) -> &'static str {
        match self {
            Self::Responses => "responses.stream.event",
            Self::ChatCompletions => "chat.completions.stream.event",
        }
    }

    pub(super) fn url(self, cfg: &Config) -> Result<String, String> {
        match self {
            Self::Responses => request::responses_url(&cfg.api_base),
            Self::ChatCompletions => request::chat_completions_url(&cfg.api_base),
        }
    }

    fn apply_headers(self, mut builder: RequestBuilder, stream: bool) -> RequestBuilder {
        builder = builder
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT_ENCODING, "identity");

        if stream {
            builder = builder.header(ACCEPT, "text/event-stream");
        }

        if self == Self::Responses {
            builder = builder.header("OpenAI-Beta", "responses=v1");
        }

        builder
    }

    fn status_error(self, status_code: u16, body: &str) -> ApiAttemptError {
        match self {
            Self::Responses => ApiAttemptError {
                should_fallback: response::should_fallback_from_responses(status_code, body),
                message: format!(
                    "responses request failed with status {}: {}",
                    status_code,
                    truncate_debug_body(body)
                ),
            },
            Self::ChatCompletions => ApiAttemptError {
                should_fallback: false,
                message: format!(
                    "chat completion request failed with status {}: {}",
                    status_code,
                    truncate_debug_body(body)
                ),
            },
        }
    }
}

pub(super) async fn collect_json_sse_events(
    response: Response,
    endpoint: TransportEndpoint,
    mut on_event: impl FnMut(Value) -> Result<(), String>,
) -> Result<(), String> {
    let mut last_event_summary = None;

    sse::collect_sse_events(response, |payload| {
        if payload == "[DONE]" {
            return Ok(false);
        }

        let event: Value = serde_json::from_str(payload).map_err(|err| {
            format!(
                "stream failed: invalid {} event JSON: {err}",
                endpoint.stream_event_label()
            )
        })?;
        last_event_summary = Some(response::summarize_stream_event(&event));
        on_event(event)?;

        Ok(true)
    })
    .await
    .map(|_| ())
    .map_err(|message| append_last_event_context(message, last_event_summary.as_deref()))
}

pub(super) async fn decode_json_response(
    endpoint: &str,
    response: Response,
    debug_enabled: bool,
) -> Result<Value, String> {
    let body = response.text().await.map_err(|err| err.to_string())?;
    let payload: Value = serde_json::from_str(&body)
        .map_err(|err| format!("failed to deserialize api response: {err} content:{body}"))?;
    if debug_enabled {
        response::log_json_payload(endpoint, &payload, true);
    }
    Ok(payload)
}

pub(super) async fn execute_request_with_http<T: Serialize>(
    http_client: &reqwest::Client,
    cfg: &Config,
    endpoint: TransportEndpoint,
    request: &T,
    stream: bool,
) -> Result<Response, ApiAttemptError> {
    let builder = build_api_request(http_client, cfg, endpoint, request, stream)
        .map_err(api_attempt_error)?;
    let response = builder.send().await.map_err(|err| ApiAttemptError {
        message: err.to_string(),
        should_fallback: false,
    })?;

    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<failed to read error body>".to_string());
    Err(endpoint.status_error(status.as_u16(), &body))
}

pub(super) fn log_request(
    cfg: &Config,
    endpoint: TransportEndpoint,
    stream: bool,
    debug_enabled: bool,
) {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream={} byot=true",
            endpoint
                .url(cfg)
                .unwrap_or_else(|_| "<invalid api base>".to_string()),
            cfg.model,
            stream,
        );
    }
}

pub(super) fn api_attempt_error(message: String) -> ApiAttemptError {
    ApiAttemptError {
        message,
        should_fallback: false,
    }
}

fn build_api_request<T: Serialize>(
    http_client: &reqwest::Client,
    cfg: &Config,
    endpoint: TransportEndpoint,
    request: &T,
    stream: bool,
) -> Result<RequestBuilder, String> {
    let builder = endpoint
        .apply_headers(http_client.post(endpoint.url(cfg)?), stream)
        .timeout(cfg.timeout);

    Ok(apply_auth(
        builder.json(&request_body(
            cfg,
            request,
            stream,
            endpoint.endpoint_kind(),
        )?),
        cfg,
    ))
}

fn request_body<T: Serialize>(
    cfg: &Config,
    request: &T,
    stream: bool,
    endpoint_kind: EndpointKind,
) -> Result<Value, String> {
    let mut body = serde_json::to_value(request).map_err(|err| err.to_string())?;
    if stream {
        body["stream"] = Value::Bool(true);
    }
    apply_provider_body_defaults(cfg, endpoint_kind, &mut body);
    Ok(body)
}

fn append_last_event_context(message: String, last_event_summary: Option<&str>) -> String {
    match last_event_summary {
        Some(summary) => format!("{message}; last parsed event: {summary}"),
        None => message,
    }
}

#[cfg(test)]
mod tests {
    use super::{TransportEndpoint, request_body};
    use crate::config::{Config, Provider, default_redaction_rules};
    use async_openai::types::responses::{CreateResponseArgs, InputParam};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn responses_body_sets_default_reasoning_effort_to_none() {
        let cfg = sample_config();
        let request = CreateResponseArgs::default()
            .model("gpt-4.1-mini")
            .input(InputParam::Text("hello".to_string()))
            .build()
            .expect("request");

        let body = request_body(
            &cfg,
            &request,
            false,
            TransportEndpoint::Responses.endpoint_kind(),
        )
        .expect("body");

        assert_eq!(body["reasoning"], json!({ "effort": "none" }));
    }

    #[test]
    fn responses_body_preserves_explicit_reasoning() {
        let cfg = sample_config();
        let body = request_body(
            &cfg,
            &json!({
                "model": "gpt-4.1-mini",
                "reasoning": { "effort": "high" }
            }),
            false,
            TransportEndpoint::Responses.endpoint_kind(),
        )
        .expect("body");

        assert_eq!(body["reasoning"], json!({ "effort": "high" }));
    }

    fn sample_config() -> Config {
        Config {
            provider: Provider::OpenAiCompatible,
            api_base: "https://ai.cloud1ful.com/v1".to_string(),
            api_key: "token".to_string(),
            model: "gpt-4.1-mini".to_string(),
            confirm_commit: true,
            open_editor: false,
            enable_fallback: false,
            redact_secrets: true,
            redaction_rules: default_redaction_rules(),
            show_timing: true,
            use_env_proxy: false,
            timeout: Duration::from_secs(15),
            max_diff_tokens: 16_000,
            max_diff_tokens_explicit: false,
            model_context_tokens: None,
            reasoning_effort: crate::config::ReasoningEffort::None,
            suppress_diff_dirs: vec!["sqlx".to_string()],
        }
    }
}
