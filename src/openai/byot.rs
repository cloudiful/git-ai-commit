use super::{ApiAttemptError, StreamRenderer, apply_auth, request, response};
use crate::config::Config;
use crate::provider_common::{new_http_client, new_streaming_http_client, truncate_debug_body};
use async_openai::types::chat::CreateChatCompletionRequest;
use async_openai::types::responses::CreateResponse;
use futures::StreamExt;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_TYPE, HeaderMap};
use reqwest::{RequestBuilder, Response};
use serde::Serialize;
use serde_json::Value;

pub(super) async fn run_responses_stream_once(
    cfg: &Config,
    request: &CreateResponse,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=true byot=true",
            request::responses_url(&cfg.api_base),
            cfg.model,
        );
    }

    let client = new_streaming_http_client(cfg).map_err(|message| ApiAttemptError {
        message,
        should_fallback: false,
    })?;
    let response = execute_responses_request_with_http(&client, cfg, request, true).await?;
    collect_responses_stream(response, renderer, debug_enabled).await
}

pub(super) async fn run_responses_non_stream_once(
    cfg: &Config,
    request: &CreateResponse,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=false byot=true",
            request::responses_url(&cfg.api_base),
            cfg.model,
        );
    }

    let client = new_http_client(cfg).map_err(|message| ApiAttemptError {
        message,
        should_fallback: false,
    })?;
    let response = execute_responses_request_with_http(&client, cfg, request, false).await?;
    let payload = decode_json_response("responses", response, debug_enabled)
        .await
        .map_err(|message| ApiAttemptError {
            message,
            should_fallback: false,
        })?;

    response::extract_response_text(payload, debug_enabled).map_err(|message| ApiAttemptError {
        should_fallback: response::should_fallback_from_empty_responses_payload(&message),
        message,
    })
}

pub(super) async fn run_chat_stream_once(
    cfg: &Config,
    request: &CreateChatCompletionRequest,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, String> {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=true byot=true",
            request::chat_completions_url(&cfg.api_base),
            cfg.model,
        );
    }

    let client = new_streaming_http_client(cfg)?;
    let response = execute_chat_request_with_http(&client, cfg, request, true).await?;
    collect_chat_stream(response, renderer, debug_enabled).await
}

pub(super) async fn run_chat_non_stream_once(
    cfg: &Config,
    request: &CreateChatCompletionRequest,
    debug_enabled: bool,
) -> Result<String, String> {
    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=false byot=true",
            request::chat_completions_url(&cfg.api_base),
            cfg.model,
        );
    }

    let client = new_http_client(cfg)?;
    let response = execute_chat_request_with_http(&client, cfg, request, false).await?;
    let payload = decode_json_response("chat.completions", response, debug_enabled).await?;
    response::extract_chat_message(payload, debug_enabled)
}

pub(super) async fn diagnose_raw_responses_stream(cfg: &Config, request: &CreateResponse) {
    let client = match new_streaming_http_client(cfg) {
        Ok(client) => client,
        Err(err) => {
            eprintln!(
                "git-ai-commit: provider debug: raw responses stream diagnose skipped: {}",
                err
            );
            return;
        }
    };

    let response = match execute_responses_request_with_http(&client, cfg, request, true).await {
        Ok(response) => response,
        Err(err) => {
            eprintln!(
                "git-ai-commit: provider debug: raw responses stream diagnose request failed: {}",
                err.message
            );
            return;
        }
    };

    let status = response.status();
    let headers = format_headers(response.headers());
    eprintln!(
        "git-ai-commit: provider debug: raw responses stream diagnose handshake status={} headers={}",
        status, headers
    );

    let mut byte_stream = response.bytes_stream();
    let mut chunk_count = 0usize;
    let mut total_bytes = 0usize;
    let mut tail = Vec::new();

    while let Some(chunk) = byte_stream.next().await {
        match chunk {
            Ok(bytes) => {
                chunk_count += 1;
                total_bytes += bytes.len();
                push_tail_bytes(&mut tail, bytes.as_ref(), 4096);
            }
            Err(err) => {
                eprintln!(
                    "git-ai-commit: provider debug: raw responses stream diagnose read error after chunks={} bytes={}: {}",
                    chunk_count, total_bytes, err
                );
                eprintln!(
                    "git-ai-commit: provider debug: raw responses stream diagnose tail utf8:\n{}",
                    String::from_utf8_lossy(&tail)
                );
                eprintln!(
                    "git-ai-commit: provider debug: raw responses stream diagnose tail hex:\n{}",
                    format_hex(&tail)
                );
                return;
            }
        }
    }

    eprintln!(
        "git-ai-commit: provider debug: raw responses stream diagnose completed without transport error; chunks={} bytes={}",
        chunk_count, total_bytes
    );
    if !tail.is_empty() {
        eprintln!(
            "git-ai-commit: provider debug: raw responses stream diagnose final tail utf8:\n{}",
            String::from_utf8_lossy(&tail)
        );
    }
}

async fn collect_responses_stream(
    response: Response,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    let mut accumulator = response::ResponseTextAccumulator::default();
    let mut error_message = None;

    collect_sse_events(response, |payload| {
        if payload == "[DONE]" {
            return Ok(false);
        }

        let event: Value = serde_json::from_str(payload)
            .map_err(|err| format!("stream failed: invalid responses event JSON: {err}"))?;
        if let Some(message) = response::append_response_stream_event_text(
            &event,
            renderer,
            &mut accumulator,
            debug_enabled,
        )? {
            error_message = Some(message);
        }

        Ok(true)
    })
    .await
    .map_err(|message| ApiAttemptError {
        should_fallback: false,
        message,
    })?;

    if !accumulator.content().trim().is_empty() {
        return Ok(crate::message::sanitize_message(accumulator.content()));
    }

    let message =
        error_message.unwrap_or_else(|| "responses request returned no output text".to_string());
    Err(ApiAttemptError {
        should_fallback: response::should_fallback_from_responses_message(&message)
            || response::should_fallback_from_empty_responses_payload(&message),
        message,
    })
}

async fn collect_chat_stream(
    response: Response,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, String> {
    let mut content = String::new();

    collect_sse_events(response, |payload| {
        if payload == "[DONE]" {
            return Ok(false);
        }

        let event: Value = serde_json::from_str(payload)
            .map_err(|err| format!("stream failed: invalid chat event JSON: {err}"))?;
        if debug_enabled {
            response::log_json_payload("chat.completions.stream.event", &event, true);
        }
        if let Some(message) = response::extract_error_message(&event) {
            return Err(message);
        }

        let delta = response::extract_chat_stream_delta(&event);
        if !delta.is_empty() {
            renderer.push(&delta).map_err(|err| err.to_string())?;
            content.push_str(&delta);
        }

        Ok(true)
    })
    .await?;

    let sanitized = crate::message::sanitize_message(&content);
    if sanitized.is_empty() {
        return Err("chat completion returned no stream chunks".to_string());
    }
    Ok(sanitized)
}

async fn collect_sse_events(
    response: Response,
    mut on_payload: impl FnMut(&str) -> Result<bool, String>,
) -> Result<(), String> {
    let mut byte_stream = response.bytes_stream();
    let mut buffer = Vec::new();

    while let Some(chunk) = byte_stream.next().await {
        let chunk = chunk.map_err(|err| format!("stream failed: {err}"))?;
        buffer.extend_from_slice(&chunk);

        while let Some(payload) = take_next_sse_payload(&mut buffer)? {
            if !on_payload(&payload)? {
                return Ok(());
            }
        }
    }

    while let Some(payload) = take_next_sse_payload_with_eof(&mut buffer)? {
        if !on_payload(&payload)? {
            return Ok(());
        }
    }

    Ok(())
}

async fn decode_json_response(
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

async fn execute_responses_request_with_http(
    http_client: &reqwest::Client,
    cfg: &Config,
    request: &CreateResponse,
    stream: bool,
) -> Result<Response, ApiAttemptError> {
    let builder = build_responses_request(http_client, cfg, request, stream)
        .map_err(|message| ApiAttemptError {
            message,
            should_fallback: false,
        })?;
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
    Err(ApiAttemptError {
        should_fallback: response::should_fallback_from_responses(status.as_u16(), &body),
        message: format!(
            "responses request failed with status {}: {}",
            status.as_u16(),
            truncate_debug_body(&body)
        ),
    })
}

async fn execute_chat_request_with_http(
    http_client: &reqwest::Client,
    cfg: &Config,
    request: &CreateChatCompletionRequest,
    stream: bool,
) -> Result<Response, String> {
    let builder = build_chat_request(http_client, cfg, request, stream)?;
    let response = builder.send().await.map_err(|err| err.to_string())?;

    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<failed to read error body>".to_string());
    Err(format!(
        "chat completion request failed with status {}: {}",
        status.as_u16(),
        truncate_debug_body(&body)
    ))
}

fn build_responses_request(
    http_client: &reqwest::Client,
    cfg: &Config,
    request: &CreateResponse,
    stream: bool,
) -> Result<RequestBuilder, String> {
    let mut builder = http_client
        .post(request::responses_url(&cfg.api_base))
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT_ENCODING, "identity")
        .header("OpenAI-Beta", "responses=v1")
        .timeout(cfg.timeout);

    if stream {
        builder = builder.header(ACCEPT, "text/event-stream");
    }

    Ok(apply_auth(builder_json(builder, request, stream)?, cfg))
}

fn build_chat_request(
    http_client: &reqwest::Client,
    cfg: &Config,
    request: &CreateChatCompletionRequest,
    stream: bool,
) -> Result<RequestBuilder, String> {
    let mut builder = http_client
        .post(request::chat_completions_url(&cfg.api_base))
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT_ENCODING, "identity")
        .timeout(cfg.timeout);

    if stream {
        builder = builder.header(ACCEPT, "text/event-stream");
    }

    Ok(apply_auth(builder_json(builder, request, stream)?, cfg))
}

fn builder_json<T: Serialize>(
    builder: RequestBuilder,
    request: &T,
    stream: bool,
) -> Result<RequestBuilder, String> {
    let mut body = serde_json::to_value(request).map_err(|err| err.to_string())?;
    if stream {
        body["stream"] = Value::Bool(true);
    }
    Ok(builder.json(&body))
}

fn take_next_sse_payload(buffer: &mut Vec<u8>) -> Result<Option<String>, String> {
    take_next_sse_payload_inner(buffer, false)
}

fn take_next_sse_payload_with_eof(buffer: &mut Vec<u8>) -> Result<Option<String>, String> {
    take_next_sse_payload_inner(buffer, true)
}

fn take_next_sse_payload_inner(buffer: &mut Vec<u8>, flush_eof: bool) -> Result<Option<String>, String> {
    let Some((event_len, separator_len)) = find_sse_event_boundary(buffer).or_else(|| {
        if flush_eof && !buffer.is_empty() {
            Some((buffer.len(), 0))
        } else {
            None
        }
    }) else {
        return Ok(None);
    };

    let event_bytes = buffer[..event_len].to_vec();
    buffer.drain(..event_len + separator_len);
    let event = std::str::from_utf8(&event_bytes)
        .map_err(|err| format!("stream failed: invalid utf8 SSE payload: {err}"))?;

    let mut data_lines = Vec::new();
    for line in event.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(payload) = line.strip_prefix("data:") {
            data_lines.push(payload.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        return Ok(Some(String::new()));
    }

    Ok(Some(data_lines.join("\n")))
}

fn find_sse_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut idx = 0usize;
    while idx < buffer.len() {
        if idx + 3 < buffer.len() && &buffer[idx..idx + 4] == b"\r\n\r\n" {
            return Some((idx, 4));
        }
        if idx + 1 < buffer.len() && &buffer[idx..idx + 2] == b"\n\n" {
            return Some((idx, 2));
        }
        idx += 1;
    }
    None
}

fn format_headers(headers: &HeaderMap) -> String {
    let mut parts = Vec::new();
    for (name, value) in headers {
        let value = value.to_str().unwrap_or("<non-utf8>");
        parts.push(format!("{}={}", name.as_str(), value));
    }
    parts.join(", ")
}

fn push_tail_bytes(buffer: &mut Vec<u8>, chunk: &[u8], limit: usize) {
    if chunk.is_empty() {
        return;
    }

    buffer.extend_from_slice(chunk);
    if buffer.len() > limit {
        let overflow = buffer.len() - limit;
        buffer.drain(0..overflow);
    }
}

fn format_hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (idx, byte) in bytes.iter().enumerate() {
        if idx > 0 {
            if idx % 16 == 0 {
                out.push('\n');
            } else {
                out.push(' ');
            }
        }
        out.push_str(&format!("{:02x}", byte));
    }
    out
}
