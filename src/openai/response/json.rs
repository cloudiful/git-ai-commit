use crate::message::sanitize_message;

use super::models::{ChatCompletionResponse, ResponsesApiResponse};
use super::{provider_error_message, provider_status_error};

pub(super) fn parse_responses_api_response(status_code: u16, body: &str) -> Result<String, String> {
    let parsed: ResponsesApiResponse = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_err) if status_code >= 400 => {
            return Err(format!("responses request failed: {}", body.trim()));
        }
        Err(err) => return Err(format!("invalid responses payload: {err}")),
    };

    if status_code >= 400 {
        return Err(provider_status_error(
            status_code,
            provider_error_message(parsed.error),
            |status_code| format!("responses request failed with status {status_code}"),
        ));
    }

    if let Some(output_text) = parsed.output_text {
        let sanitized = sanitize_message(&output_text);
        if !sanitized.is_empty() {
            return Ok(sanitized);
        }
    }

    let mut aggregated = String::new();
    for item in parsed.output {
        for part in item.content {
            if let Some(text) = part.text {
                aggregated.push_str(&text);
            }
        }
    }

    let sanitized = sanitize_message(&aggregated);
    if sanitized.is_empty() {
        return Err("responses request returned no output text".to_string());
    }

    Ok(sanitized)
}

pub(super) fn parse_json_chat_completion(status_code: u16, body: &str) -> Result<String, String> {
    let parsed: ChatCompletionResponse = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_err) if status_code >= 400 => {
            return Err(format!("chat completion failed: {}", body.trim()));
        }
        Err(err) => return Err(err.to_string()),
    };

    if status_code >= 400 {
        return Err(provider_status_error(
            status_code,
            provider_error_message(parsed.error),
            |status_code| format!("chat completion failed with status {status_code}"),
        ));
    }

    let choice = parsed
        .choices
        .first()
        .ok_or_else(|| "chat completion returned no choices".to_string())?;
    Ok(sanitize_message(&choice.message.content))
}

pub(super) fn should_fallback_from_responses(status_code: u16, error_message: &str) -> bool {
    if matches!(status_code, 404 | 405 | 415 | 501) {
        return true;
    }

    if status_code != 400 && status_code != 422 {
        return false;
    }

    let combined = error_message.to_ascii_lowercase();
    [
        "unsupported",
        "not supported",
        "unrecognized request url",
        "unknown request url",
        "unknown path",
        "invalid url",
        "not found",
        "no route",
        "not implemented",
        "/v1/responses",
    ]
    .iter()
    .any(|needle| combined.contains(needle))
}
