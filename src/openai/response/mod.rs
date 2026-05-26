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
use std::collections::HashMap;

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

pub(super) fn should_fallback_from_responses(status_code: u16, error_message: &str) -> bool {
    if matches!(status_code, 405 | 415 | 501) {
        return true;
    }

    if !matches!(status_code, 400 | 404 | 422) {
        return false;
    }

    has_responses_endpoint_unsupported_signal(error_message)
}

pub(super) fn should_fallback_from_responses_message(error_message: &str) -> bool {
    has_responses_endpoint_unsupported_signal(error_message)
}

pub(super) fn should_fallback_from_empty_responses_payload(error_message: &str) -> bool {
    matches!(
        error_message,
        "responses request returned no output text"
    )
}

pub(super) fn should_retry_without_stream_message(error_message: &str) -> bool {
    let lowered = error_message.to_ascii_lowercase();
    [
        "stream failed",
        "eventstream error",
        "transport error",
        "error decoding response body",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

pub(super) fn append_response_stream_event_text(
    event: ResponseStreamEvent,
    renderer: &mut StreamRenderer,
    accumulator: &mut ResponseTextAccumulator,
    debug_enabled: bool,
) -> Result<Option<String>, String> {
    if debug_enabled {
        log_response_payload("responses.stream.event", &event, true);
    }

    match event {
        ResponseStreamEvent::ResponseOutputTextDelta(event) => {
            accumulator.push_delta(
                &event.item_id,
                event.output_index,
                event.content_index,
                &event.delta,
                renderer,
            )?;
            Ok(None)
        }
        ResponseStreamEvent::ResponseOutputTextDone(event) => {
            accumulator.push_slot_text_if_missing(
                &event.item_id,
                event.output_index,
                event.content_index,
                &event.text,
                renderer,
            )?;
            Ok(None)
        }
        ResponseStreamEvent::ResponseContentPartDone(event) => {
            accumulator.push_output_content_if_missing(
                &event.item_id,
                event.output_index,
                event.content_index,
                &event.part,
                renderer,
            )?;
            Ok(None)
        }
        ResponseStreamEvent::ResponseOutputItemDone(event) => {
            accumulator.push_output_item_if_missing(event.output_index, &event.item, renderer)?;
            Ok(None)
        }
        ResponseStreamEvent::ResponseError(event) => Ok(Some(event.message)),
        _ => Ok(None),
    }
}

fn has_responses_endpoint_unsupported_signal(error_message: &str) -> bool {
    let lowered = error_message.to_ascii_lowercase();
    [
        "unsupported",
        "not supported",
        "unrecognized request url",
        "unknown request url",
        "unknown path",
        "invalid url",
        "no route",
        "not implemented",
        "404 page not found",
        "method not allowed",
        "unsupported media type",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

#[derive(Default)]
pub(super) struct ResponseTextAccumulator {
    content: String,
    slots: HashMap<ResponseTextSlotKey, ResponseTextSlot>,
}

impl ResponseTextAccumulator {
    pub(super) fn content(&self) -> &str {
        &self.content
    }

    fn push_delta(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        text: &str,
        renderer: &mut StreamRenderer,
    ) -> Result<(), String> {
        self.push_text(
            ResponseTextSlotKey::new(item_id, output_index, content_index),
            text,
            renderer,
        )
    }

    fn push_slot_text_if_missing(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        text: &str,
        renderer: &mut StreamRenderer,
    ) -> Result<(), String> {
        self.push_text_if_missing(
            ResponseTextSlotKey::new(item_id, output_index, content_index),
            text,
            renderer,
        )
    }

    fn push_output_content_if_missing(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        part: &OutputContent,
        renderer: &mut StreamRenderer,
    ) -> Result<(), String> {
        if let OutputContent::OutputText(text) = part {
            self.push_text_if_missing(
                ResponseTextSlotKey::new(item_id, output_index, content_index),
                &text.text,
                renderer,
            )?;
        }
        Ok(())
    }

    fn push_output_item_if_missing(
        &mut self,
        output_index: u32,
        item: &OutputItem,
        renderer: &mut StreamRenderer,
    ) -> Result<(), String> {
        let OutputItem::Message(message) = item else {
            return Ok(());
        };

        for (content_index, part) in message.content.iter().enumerate() {
            if let OutputMessageContent::OutputText(text) = part {
                self.push_text_if_missing(
                    ResponseTextSlotKey::new(&message.id, output_index, content_index as u32),
                    &text.text,
                    renderer,
                )?;
            }
        }
        Ok(())
    }

    fn push_text(
        &mut self,
        key: ResponseTextSlotKey,
        text: &str,
        renderer: &mut StreamRenderer,
    ) -> Result<(), String> {
        if text.is_empty() {
            return Ok(());
        }

        renderer.push(text).map_err(|err| err.to_string())?;
        self.content.push_str(text);
        self.slots.entry(key).or_default().emitted = true;
        Ok(())
    }

    fn push_text_if_missing(
        &mut self,
        key: ResponseTextSlotKey,
        text: &str,
        renderer: &mut StreamRenderer,
    ) -> Result<(), String> {
        if text.is_empty() {
            return Ok(());
        }

        let slot = self.slots.entry(key).or_default();
        if slot.emitted {
            return Ok(());
        }

        renderer.push(text).map_err(|err| err.to_string())?;
        self.content.push_str(text);
        slot.emitted = true;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ResponseTextSlotKey {
    item_id: String,
    output_index: u32,
    content_index: u32,
}

impl ResponseTextSlotKey {
    fn new(item_id: &str, output_index: u32, content_index: u32) -> Self {
        Self {
            item_id: item_id.to_string(),
            output_index,
            content_index,
        }
    }
}

#[derive(Default)]
struct ResponseTextSlot {
    emitted: bool,
}
