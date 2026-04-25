mod json;
mod models;
mod streaming;

#[cfg(test)]
mod tests;

use reqwest::blocking::Response;

use self::models::ProviderError;
use super::stream::{StreamRenderer, is_event_stream};

pub(super) fn parse_responses_response(
    status_code: u16,
    content_type: &str,
    response: Response,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, String> {
    if is_event_stream(content_type) {
        streaming::parse_streaming_responses_api_response(status_code, response, renderer)
    } else {
        let body = response.text().map_err(|err| err.to_string())?;
        log_provider_error_details("responses", status_code, content_type, &body, debug_enabled);
        parse_responses_api_response(status_code, &body)
    }
}

pub(super) fn parse_chat_completion_response(
    status_code: u16,
    content_type: &str,
    response: Response,
    renderer: &mut StreamRenderer,
    debug_enabled: bool,
) -> Result<String, String> {
    if is_event_stream(content_type) {
        streaming::parse_streaming_chat_completion(status_code, response, renderer)
    } else {
        let body = response.text().map_err(|err| err.to_string())?;
        log_provider_error_details(
            "chat.completions",
            status_code,
            content_type,
            &body,
            debug_enabled,
        );
        parse_json_chat_completion(status_code, &body)
    }
}

pub(super) fn parse_responses_api_response(status_code: u16, body: &str) -> Result<String, String> {
    json::parse_responses_api_response(status_code, body)
}

pub(super) fn parse_json_chat_completion(status_code: u16, body: &str) -> Result<String, String> {
    json::parse_json_chat_completion(status_code, body)
}

pub(super) fn should_fallback_from_responses(status_code: u16, error_message: &str) -> bool {
    json::should_fallback_from_responses(status_code, error_message)
}

fn provider_error_message(error: Option<ProviderError>) -> Option<String> {
    error
        .map(|error| error.message)
        .filter(|message| !message.trim().is_empty())
}

fn provider_status_error(
    status_code: u16,
    provider_error: Option<String>,
    default_message: impl FnOnce(u16) -> String,
) -> String {
    provider_error.unwrap_or_else(|| default_message(status_code))
}

fn log_provider_error_details(
    endpoint: &str,
    status_code: u16,
    content_type: &str,
    body: &str,
    debug_enabled: bool,
) {
    if !debug_enabled || status_code < 400 {
        return;
    }

    eprintln!(
        "git-ai-commit: provider debug: {} failed with status {} content-type={}",
        endpoint, status_code, content_type
    );
    eprintln!(
        "git-ai-commit: provider debug: response body: {}",
        truncate_single_line(body, 1_000)
    );
}

fn truncate_single_line(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
pub(super) fn collect_streaming_chat_completion<R: std::io::BufRead>(
    status_code: u16,
    reader: R,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    streaming::collect_streaming_chat_completion(status_code, reader, renderer)
}

#[cfg(test)]
pub(super) fn collect_streaming_responses_api_response<R: std::io::BufRead>(
    status_code: u16,
    reader: R,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    streaming::collect_streaming_responses_api_response(status_code, reader, renderer)
}
