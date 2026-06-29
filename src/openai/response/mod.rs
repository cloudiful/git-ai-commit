#[cfg(test)]
mod tests;

use crate::message::sanitize_message;
use crate::openai::StreamRenderer;
use crate::provider_common::truncate_debug_body;
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn extract_response_text(response: Value, debug_enabled: bool) -> Result<String, String> {
    log_response_payload("responses", &response, debug_enabled);

    let message = response
        .get("output_text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| extract_response_output_text(&response))
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

pub(super) fn extract_chat_message(response: Value, debug_enabled: bool) -> Result<String, String> {
    log_response_payload("chat.completions", &response, debug_enabled);

    let content = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(value_as_text)
        .map(|text| sanitize_message(&text))
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "chat completion returned no choices".to_string())?;
    Ok(content)
}

pub(super) fn log_response_payload<T: serde::Serialize>(
    endpoint: &str,
    payload: &T,
    debug_enabled: bool,
) {
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

pub(super) fn log_json_payload(endpoint: &str, payload: &Value, debug_enabled: bool) {
    log_response_payload(endpoint, payload, debug_enabled);
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
    event: &Value,
    renderer: &mut StreamRenderer,
    accumulator: &mut ResponseTextAccumulator,
    debug_enabled: bool,
) -> Result<Option<String>, String> {
    if debug_enabled {
        log_response_payload("responses.stream.event", &event, true);
    }

    match event.get("type").and_then(Value::as_str) {
        Some("response.output_text.delta") => {
            accumulator.push_delta(
                required_str(event, "item_id")?,
                required_u32(event, "output_index")?,
                required_u32(event, "content_index")?,
                required_str(event, "delta")?,
                renderer,
            )?;
            Ok(None)
        }
        Some("response.output_text.done") => {
            accumulator.push_slot_text_if_missing(
                required_str(event, "item_id")?,
                required_u32(event, "output_index")?,
                required_u32(event, "content_index")?,
                required_str(event, "text")?,
                renderer,
            )?;
            Ok(None)
        }
        Some("response.content_part.done") => {
            accumulator.push_output_text_if_missing_from_value(
                required_str(event, "item_id")?,
                required_u32(event, "output_index")?,
                required_u32(event, "content_index")?,
                event.get("part")
                    .ok_or_else(|| "responses stream event missing part".to_string())?,
                renderer,
            )?;
            Ok(None)
        }
        Some("response.output_item.done") => {
            accumulator.push_output_item_if_missing_from_value(
                required_u32(event, "output_index")?,
                event.get("item")
                    .ok_or_else(|| "responses stream event missing item".to_string())?,
                renderer,
            )?;
            Ok(None)
        }
        Some("error") | Some("response.error") => Ok(extract_error_message(event)),
        _ => Ok(None),
    }
}

pub(super) fn extract_error_message(event: &Value) -> Option<String> {
    event
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            event.get("error").and_then(|error| {
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| Some(error.to_string()))
            })
        })
}

pub(super) fn extract_chat_stream_delta(event: &Value) -> String {
    event.get("choices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|choice| {
            choice
                .get("delta")
                .and_then(|delta| delta.get("content"))
                .and_then(value_as_text)
        })
        .collect()
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

    fn push_output_text_if_missing_from_value(
        &mut self,
        item_id: &str,
        output_index: u32,
        content_index: u32,
        part: &Value,
        renderer: &mut StreamRenderer,
    ) -> Result<(), String> {
        if part.get("type").and_then(Value::as_str) != Some("output_text") {
            return Ok(());
        }

        if let Some(text) = part.get("text").and_then(Value::as_str) {
            self.push_text_if_missing(
                ResponseTextSlotKey::new(item_id, output_index, content_index),
                text,
                renderer,
            )?;
        }
        Ok(())
    }

    fn push_output_item_if_missing_from_value(
        &mut self,
        output_index: u32,
        item: &Value,
        renderer: &mut StreamRenderer,
    ) -> Result<(), String> {
        if item.get("type").and_then(Value::as_str) != Some("message") {
            return Ok(());
        }

        let Some(message_id) = item.get("id").and_then(Value::as_str) else {
            return Ok(());
        };
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            return Ok(());
        };

        for (content_index, part) in content.iter().enumerate() {
            if part.get("type").and_then(Value::as_str) != Some("output_text") {
                continue;
            }
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                self.push_text_if_missing(
                    ResponseTextSlotKey::new(message_id, output_index, content_index as u32),
                    text,
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

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("responses stream event missing string field {key}"))
}

fn required_u32(value: &Value, key: &str) -> Result<u32, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| format!("responses stream event missing integer field {key}"))
}

fn extract_response_output_text(response: &Value) -> Option<String> {
    let output = response.get("output")?.as_array()?;
    let mut out = String::new();

    for item in output {
        if item.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(parts) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for part in parts {
            if part.get("type").and_then(Value::as_str) != Some("output_text") {
                continue;
            }
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn value_as_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(text) = item
                    .get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.as_str())
                {
                    out.push_str(text);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        _ => None,
    }
}
