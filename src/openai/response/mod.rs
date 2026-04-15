mod json;
mod models;
mod streaming;

#[cfg(test)]
mod tests;

use reqwest::blocking::Response;

use super::stream::{StreamRenderer, is_event_stream};
use self::models::ProviderError;

pub(super) fn parse_responses_response(
    status_code: u16,
    content_type: &str,
    response: Response,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    if is_event_stream(content_type) {
        streaming::parse_streaming_responses_api_response(status_code, response, renderer)
    } else {
        let body = response.text().map_err(|err| err.to_string())?;
        parse_responses_api_response(status_code, &body)
    }
}

pub(super) fn parse_chat_completion_response(
    status_code: u16,
    content_type: &str,
    response: Response,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    if is_event_stream(content_type) {
        streaming::parse_streaming_chat_completion(status_code, response, renderer)
    } else {
        let body = response.text().map_err(|err| err.to_string())?;
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

#[cfg(test)]
pub(super) fn collect_streaming_chat_completion<R: std::io::BufRead>(
    status_code: u16,
    reader: R,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    streaming::collect_streaming_chat_completion(status_code, reader, renderer)
}
