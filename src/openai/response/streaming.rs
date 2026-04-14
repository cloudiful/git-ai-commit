use crate::message::sanitize_message;
use reqwest::blocking::Response;
use std::io::{BufRead, BufReader};

use super::models::{ChatCompletionChunk, ResponsesStreamEvent};
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

fn collect_streaming_responses_api_response<R: BufRead>(
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

        if let Some(error) = event.error {
            if !error.message.trim().is_empty() {
                provider_error = error.message;
            }
        }
        if let Some(message) = event.message {
            if event.event_type == "error" && !message.trim().is_empty() {
                provider_error = message;
            }
        }

        match event.event_type.as_str() {
            "response.output_text.delta" => {
                if let Some(delta) = event.delta {
                    renderer.push(&delta).map_err(|err| err.to_string())?;
                    content.push_str(&delta);
                }
            }
            "response.output_text.done" => {
                if content.is_empty() {
                    if let Some(text) = event.text {
                        renderer.push(&text).map_err(|err| err.to_string())?;
                        content.push_str(&text);
                    }
                }
            }
            _ => {}
        }

        Ok(true)
    })?;

    if status_code >= 400 {
        if !provider_error.is_empty() {
            return Err(provider_error);
        }
        return Err(format!(
            "responses request failed with status {status_code}"
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
        if let Some(error) = chunk.error {
            if !error.message.trim().is_empty() {
                provider_error = error.message;
            }
        }
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
        if !provider_error.is_empty() {
            return Err(provider_error);
        }
        return Err(format!("chat completion failed with status {status_code}"));
    }
    if !saw_chunk {
        return Err("chat completion returned no stream chunks".to_string());
    }

    Ok(sanitize_message(&content))
}
