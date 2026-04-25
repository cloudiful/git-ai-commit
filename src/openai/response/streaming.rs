use crate::message::sanitize_message;
use reqwest::blocking::Response;
use std::io::{BufRead, BufReader};

use super::models::{ChatCompletionChunk, ResponsesStreamEvent};
use super::provider_status_error;
use crate::openai::stream::{StreamRenderer, parse_sse_payloads};

pub(super) fn parse_streaming_responses_api_response(
    status_code: u16,
    response: Response,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    collect_streaming_responses_api_response(status_code, BufReader::new(response), renderer)
}

pub(super) fn parse_streaming_chat_completion(
    status_code: u16,
    response: Response,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    collect_streaming_chat_completion(status_code, BufReader::new(response), renderer)
}

pub(super) fn collect_streaming_responses_api_response<R: BufRead>(
    status_code: u16,
    reader: R,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    let mut content = String::new();
    let mut provider_error = String::new();
    let mut saw_chunk = false;

    parse_sse_payloads(reader, |payload| {
        if payload == "[DONE]" {
            return Ok(false);
        }

        saw_chunk = true;
        let event: ResponsesStreamEvent = serde_json::from_str(payload)
            .map_err(|err| format!("invalid responses stream event: {err}"))?;

        update_provider_error(&mut provider_error, event.error.map(|error| error.message));
        update_provider_error(
            &mut provider_error,
            event
                .message
                .filter(|message| event.event_type == "error" && !message.trim().is_empty()),
        );

        match event.event_type.as_str() {
            "response.output_text.delta" => {
                if let Some(delta) = event.delta {
                    renderer.push(&delta).map_err(|err| err.to_string())?;
                    content.push_str(&delta);
                }
            }
            "response.content_part.delta" => {
                if let Some(delta) = event.part.and_then(|part| part.text).or(event.delta) {
                    renderer.push(&delta).map_err(|err| err.to_string())?;
                    content.push_str(&delta);
                }
            }
            "response.output_text.done" => {
                if content.is_empty()
                    && let Some(text) = event.text
                {
                    renderer.push(&text).map_err(|err| err.to_string())?;
                    content.push_str(&text);
                }
            }
            "response.output_item.done" => {
                if content.is_empty()
                    && let Some(item) = event.item
                {
                    for part in item.content {
                        if let Some(text) = part.text {
                            renderer.push(&text).map_err(|err| err.to_string())?;
                            content.push_str(&text);
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(true)
    })?;

    if status_code >= 400 {
        return Err(provider_status_error(
            status_code,
            non_empty_string(provider_error),
            |status_code| format!("responses request failed with status {status_code}"),
        ));
    }
    if !saw_chunk {
        return Err("responses request returned no stream chunks".to_string());
    }

    Ok(sanitize_message(&content))
}

pub(super) fn collect_streaming_chat_completion<R: BufRead>(
    status_code: u16,
    reader: R,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    let mut content = String::new();
    let mut provider_error = String::new();
    let mut saw_chunk = false;

    parse_sse_payloads(reader, |payload| {
        if payload == "[DONE]" {
            return Ok(false);
        }

        saw_chunk = true;
        let chunk: ChatCompletionChunk = serde_json::from_str(payload)
            .map_err(|err| format!("invalid streaming chat completion chunk: {err}"))?;
        update_provider_error(&mut provider_error, chunk.error.map(|error| error.message));
        for choice in chunk.choices {
            let _ = choice.delta.role;
            if let Some(content_delta) = choice.delta.content {
                renderer
                    .push(&content_delta)
                    .map_err(|err| err.to_string())?;
                content.push_str(&content_delta);
            }
        }

        Ok(true)
    })?;

    if status_code >= 400 {
        return Err(provider_status_error(
            status_code,
            non_empty_string(provider_error),
            |status_code| format!("chat completion failed with status {status_code}"),
        ));
    }
    if !saw_chunk {
        return Err("chat completion returned no stream chunks".to_string());
    }

    Ok(sanitize_message(&content))
}

fn update_provider_error(target: &mut String, candidate: Option<String>) {
    if let Some(message) = candidate.filter(|message| !message.trim().is_empty()) {
        *target = message;
    }
}

fn non_empty_string(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
