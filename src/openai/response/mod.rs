#[cfg(test)]
mod tests;

use crate::message::sanitize_message;
use crate::openai::StreamRenderer;
use crate::provider_common::truncate_debug_body;
use async_openai::types::chat::{
    ChatCompletionResponseStream, CreateChatCompletionResponse,
};
use async_openai::types::responses::{
    OutputContent, OutputItem, OutputMessageContent, Response, ResponseStreamEvent,
};
use futures::StreamExt;

pub(super) fn extract_response_text(response: Response, debug_enabled: bool) -> Result<String, String> {
    log_response_payload("responses", &response, debug_enabled);

    let message = response
        .output_text()
        .map(|text| sanitize_message(&text))
        .filter(|text| !text.is_empty());

    if let Some(message) = message {
        return Ok(message);
    }

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: responses payload had no extractable output_text"
        );
    }

    Err("responses request returned no output text".to_string())
}

pub(super) fn extract_chat_message(
    response: CreateChatCompletionResponse,
    debug_enabled: bool,
) -> Result<String, String> {
    log_response_payload("chat.completions", &response, debug_enabled);

    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_deref())
        .map(sanitize_message)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "chat completion returned no choices".to_string())?;
    Ok(content)
}

fn log_response_payload<T: serde::Serialize>(endpoint: &str, payload: &T, debug_enabled: bool) {
    if !debug_enabled {
        return;
    }

    match serde_json::to_string(payload) {
        Ok(json) => {
            eprintln!(
                "git-ai-commit: provider debug: {} response body: {}",
                endpoint,
                truncate_debug_body(&json)
            );
            eprintln!(
                "git-ai-commit: provider debug: {} response body full:\n{}",
                endpoint, json
            );
        }
        Err(err) => {
            eprintln!(
                "git-ai-commit: provider debug: failed to serialize {} response body: {}",
                endpoint, err
            );
        }
    }
}

pub(super) async fn collect_chat_completion_stream(
    mut stream: ChatCompletionResponseStream,
    renderer: &mut StreamRenderer,
) -> Result<String, String> {
    let mut content = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        for choice in chunk.choices {
            if let Some(delta) = choice.delta.content {
                renderer.push(&delta).map_err(|err| err.to_string())?;
                content.push_str(&delta);
            }
        }
    }

    let sanitized = sanitize_message(&content);
    if sanitized.is_empty() {
        return Err("chat completion returned no stream chunks".to_string());
    }
    Ok(sanitized)
}

#[cfg(test)]
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

pub(super) fn should_fallback_from_responses_message(error_message: &str) -> bool {
    let lowered = error_message.to_ascii_lowercase();
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
        "404",
        "405",
        "415",
        "422",
        "501",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

pub(super) fn should_fallback_from_empty_responses_payload(error_message: &str) -> bool {
    matches!(
        error_message,
        "responses request returned no output text"
    )
}

pub(super) fn append_response_stream_event_text(
    event: ResponseStreamEvent,
    renderer: &mut StreamRenderer,
    content: &mut String,
    debug_enabled: bool,
) -> Result<Option<String>, String> {
    if debug_enabled {
        log_response_payload("responses.stream.event", &event, true);
    }

    match event {
        ResponseStreamEvent::ResponseOutputTextDelta(event) => {
            renderer.push(&event.delta).map_err(|err| err.to_string())?;
            content.push_str(&event.delta);
            Ok(None)
        }
        ResponseStreamEvent::ResponseOutputTextDone(event) => {
            if content.is_empty() {
                renderer.push(&event.text).map_err(|err| err.to_string())?;
                content.push_str(&event.text);
            }
            Ok(None)
        }
        ResponseStreamEvent::ResponseContentPartDone(event) => {
            append_output_content_text(&event.part, renderer, content)?;
            Ok(None)
        }
        ResponseStreamEvent::ResponseOutputItemDone(event) => {
            append_output_item_text(&event.item, renderer, content)?;
            Ok(None)
        }
        ResponseStreamEvent::ResponseError(event) => Ok(Some(event.message)),
        _ => Ok(None),
    }
}

fn append_output_item_text(
    item: &OutputItem,
    renderer: &mut StreamRenderer,
    content: &mut String,
) -> Result<(), String> {
    let OutputItem::Message(message) = item else {
        return Ok(());
    };

    for part in &message.content {
        if let OutputMessageContent::OutputText(text) = part {
            append_text_chunk(&text.text, renderer, content)?;
        }
    }
    Ok(())
}

fn append_output_content_text(
    part: &OutputContent,
    renderer: &mut StreamRenderer,
    content: &mut String,
) -> Result<(), String> {
    if let OutputContent::OutputText(text) = part {
        append_text_chunk(&text.text, renderer, content)?;
    }
    Ok(())
}

fn append_text_chunk(
    text: &str,
    renderer: &mut StreamRenderer,
    content: &mut String,
) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    if !content.contains(text) {
        renderer.push(text).map_err(|err| err.to_string())?;
        content.push_str(text);
    }
    Ok(())
}
