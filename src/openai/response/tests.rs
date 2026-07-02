use super::{
    ResponseTextAccumulator, append_response_stream_event_text, extract_chat_message,
    extract_response_text, should_fallback_from_responses, should_fallback_from_responses_message,
    should_retry_without_stream_message,
};
use crate::openai::{StreamOutput, StreamRenderer};
use serde_json::json;

#[test]
fn falls_back_when_responses_endpoint_is_unsupported() {
    assert!(should_fallback_from_responses(
        404,
        "responses request failed with status 404: unknown path /v1/responses",
    ));
    assert!(should_fallback_from_responses(
        400,
        "unsupported endpoint: /v1/responses is not supported",
    ));
    assert!(!should_fallback_from_responses(401, "invalid api key"));
}

#[test]
fn infers_fallback_from_sdk_error_message_when_status_missing() {
    assert!(should_fallback_from_responses_message(
        "unsupported endpoint: /v1/responses is not supported"
    ));
    assert!(should_fallback_from_responses_message(
        "responses request failed with status 404: unknown path /v1/responses"
    ));
    assert!(!should_fallback_from_responses_message("invalid api key"));
    assert!(!should_fallback_from_responses_message(
        "http error: error sending request for url (https://ai.cloud1ful.com/v1/responses)"
    ));
}

#[test]
fn retries_without_stream_for_stream_transport_decode_failures() {
    assert!(should_retry_without_stream_message(
        "stream failed: EventStream error: Transport error: error decoding response body"
    ));
    assert!(should_retry_without_stream_message(
        "EventStream error: Transport error"
    ));
    assert!(!should_retry_without_stream_message("invalid api key"));
}

#[test]
fn does_not_fallback_on_model_not_found_status_alone() {
    assert!(!should_fallback_from_responses(
        404,
        "The model `missing-model` does not exist"
    ));
}

#[test]
fn extracts_response_text_from_output_items() {
    let payload = json!({
        "output": [{
            "type": "message",
            "id": "msg_1",
            "content": [{
                "type": "output_text",
                "text": "feat: add parser"
            }]
        }]
    });

    assert_eq!(
        extract_response_text(payload, false).unwrap(),
        "feat: add parser"
    );
}

#[test]
fn extracts_chat_message_from_json_content() {
    let payload = json!({
        "choices": [{
            "message": {
                "content": "feat: add parser"
            }
        }]
    });

    assert_eq!(
        extract_chat_message(payload, false).unwrap(),
        "feat: add parser"
    );
}

#[test]
fn deduplicates_delta_done_and_output_item_events() {
    let mut renderer = StreamRenderer::new(StreamOutput::None);
    let mut accumulator = ResponseTextAccumulator::default();

    for event in [
        json!({
            "type": "response.output_text.delta",
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "feat: add parser"
        }),
        json!({
            "type": "response.output_text.done",
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "text": "feat: add parser"
        }),
        json!({
            "type": "response.content_part.done",
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": "feat: add parser"
            }
        }),
        json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "type": "message",
                "id": "msg_1",
                "content": [{
                    "type": "output_text",
                    "text": "feat: add parser"
                }]
            }
        }),
    ] {
        let result =
            append_response_stream_event_text(&event, &mut renderer, &mut accumulator, false);
        assert_eq!(result.unwrap(), None);
    }

    assert_eq!(accumulator.content(), "feat: add parser");
}

#[test]
fn backfills_done_only_stream_parts_once_in_order() {
    let mut renderer = StreamRenderer::new(StreamOutput::None);
    let mut accumulator = ResponseTextAccumulator::default();

    for event in [
        json!({
            "type": "response.content_part.done",
            "item_id": "msg_2",
            "output_index": 0,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": "refactor: rewrite provider path\n\n"
            }
        }),
        json!({
            "type": "response.content_part.done",
            "item_id": "msg_2",
            "output_index": 0,
            "content_index": 1,
            "part": {
                "type": "output_text",
                "text": "Prefer responses first and retry chat as fallback."
            }
        }),
        json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "type": "message",
                "id": "msg_2",
                "content": [
                    {
                        "type": "output_text",
                        "text": "refactor: rewrite provider path\n\n"
                    },
                    {
                        "type": "output_text",
                        "text": "Prefer responses first and retry chat as fallback."
                    }
                ]
            }
        }),
    ] {
        let result =
            append_response_stream_event_text(&event, &mut renderer, &mut accumulator, false);
        assert_eq!(result.unwrap(), None);
    }

    assert_eq!(
        accumulator.content(),
        "refactor: rewrite provider path\n\nPrefer responses first and retry chat as fallback."
    );
}
