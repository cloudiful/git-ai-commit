use super::{
    ApiAttemptError, StreamRenderer,
    http_transport::{
        TransportEndpoint, api_attempt_error, collect_json_sse_events, decode_json_response,
        execute_request_with_http, log_request,
    },
    response,
};
use crate::config::Config;
use crate::provider_common::{new_http_client, new_streaming_http_client};
use async_openai::types::chat::CreateChatCompletionRequest;
use async_openai::types::responses::CreateResponse;
use futures::StreamExt;
use reqwest::Response;
use reqwest::header::HeaderMap;

pub(super) async fn run_responses_stream_once(
    cfg: &Config,
    request: &CreateResponse,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    let endpoint = TransportEndpoint::Responses;
    log_request(cfg, endpoint, true, debug_enabled);

    let client = new_streaming_http_client(cfg).map_err(api_attempt_error)?;
    let response = execute_request_with_http(&client, cfg, endpoint, request, true).await?;
    collect_responses_stream(response, renderer, debug_enabled).await
}

pub(super) async fn run_responses_non_stream_once(
    cfg: &Config,
    request: &CreateResponse,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    let endpoint = TransportEndpoint::Responses;
    log_request(cfg, endpoint, false, debug_enabled);

    let client = new_http_client(cfg).map_err(api_attempt_error)?;
    let response = execute_request_with_http(&client, cfg, endpoint, request, false).await?;
    let payload = decode_json_response(endpoint.request_label(), response, debug_enabled)
        .await
        .map_err(api_attempt_error)?;

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
    let endpoint = TransportEndpoint::ChatCompletions;
    log_request(cfg, endpoint, true, debug_enabled);

    let client = new_streaming_http_client(cfg)?;
    let response = execute_request_with_http(&client, cfg, endpoint, request, true)
        .await
        .map_err(|err| err.message)?;
    collect_chat_stream(response, renderer, debug_enabled).await
}

pub(super) async fn run_chat_non_stream_once(
    cfg: &Config,
    request: &CreateChatCompletionRequest,
    debug_enabled: bool,
) -> Result<String, String> {
    let endpoint = TransportEndpoint::ChatCompletions;
    log_request(cfg, endpoint, false, debug_enabled);

    let client = new_http_client(cfg)?;
    let response = execute_request_with_http(&client, cfg, endpoint, request, false)
        .await
        .map_err(|err| err.message)?;
    let payload = decode_json_response(endpoint.request_label(), response, debug_enabled).await?;
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

    let endpoint = TransportEndpoint::Responses;
    let response = match execute_request_with_http(&client, cfg, endpoint, request, true).await {
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
    let mut completed_message_seen = false;

    let stream_result = collect_json_sse_events(response, TransportEndpoint::Responses, |event| {
        if let Some(message) = response::append_response_stream_event_text(
            &event,
            renderer,
            &mut accumulator,
            debug_enabled,
        )? {
            error_message = Some(message);
        }

        if response::stream_event_completes_message(&event)
            && !accumulator.content().trim().is_empty()
        {
            completed_message_seen = true;
        }

        Ok(())
    })
    .await;

    finalize_responses_stream_result(
        stream_result,
        accumulator.content(),
        completed_message_seen,
        error_message,
        debug_enabled,
    )
}

fn finalize_responses_stream_result(
    stream_result: Result<(), String>,
    content: &str,
    completed_message_seen: bool,
    error_message: Option<String>,
    debug_enabled: bool,
) -> Result<String, ApiAttemptError> {
    let sanitized = crate::message::sanitize_message(content);

    match stream_result {
        Ok(()) => {
            if !sanitized.trim().is_empty() {
                return Ok(sanitized);
            }

            let message = error_message
                .unwrap_or_else(|| "responses request returned no output text".to_string());
            Err(ApiAttemptError {
                should_fallback: response::should_fallback_from_responses_message(&message)
                    || response::should_fallback_from_empty_responses_payload(&message),
                message,
            })
        }
        Err(message) if completed_message_seen && !sanitized.trim().is_empty() => {
            if debug_enabled {
                eprintln!(
                    "git-ai-commit: provider debug: responses stream ended after a completed message; returning assembled text despite stream error: {}",
                    message
                );
            }
            Ok(sanitized)
        }
        Err(message) => Err(api_attempt_error(message)),
    }
}

async fn collect_chat_stream(
    response: Response,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, String> {
    let mut content = String::new();

    collect_json_sse_events(response, TransportEndpoint::ChatCompletions, |event| {
        if debug_enabled {
            response::log_json_payload(
                TransportEndpoint::ChatCompletions.stream_debug_label(),
                &event,
                true,
            );
        }
        if let Some(message) = response::extract_error_message(&event) {
            return Err(message);
        }

        let delta = response::extract_chat_stream_delta(&event);
        if !delta.is_empty() {
            renderer.push(&delta).map_err(|err| err.to_string())?;
            content.push_str(&delta);
        }

        Ok(())
    })
    .await?;

    let sanitized = crate::message::sanitize_message(&content);
    if sanitized.is_empty() {
        return Err("chat completion returned no stream chunks".to_string());
    }
    Ok(sanitized)
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

#[cfg(test)]
mod tests {
    use super::finalize_responses_stream_result;

    #[test]
    fn returns_message_when_stream_tail_fails_after_completed_message() {
        let result = finalize_responses_stream_result(
            Err("stream failed: error decoding response body".to_string()),
            "feat: keep completed stream output",
            true,
            None,
            false,
        )
        .expect("message should be preserved");

        assert_eq!(result, "feat: keep completed stream output");
    }

    #[test]
    fn keeps_failing_when_stream_tail_fails_without_completed_message() {
        let err = finalize_responses_stream_result(
            Err("stream failed: error decoding response body".to_string()),
            "feat: partial output",
            false,
            None,
            false,
        )
        .expect_err("partial output should not be trusted");

        assert!(err.message.contains("error decoding response body"));
    }
}
