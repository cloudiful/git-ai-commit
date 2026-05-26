use super::{
    ResponseTextAccumulator, append_response_stream_event_text, should_fallback_from_responses,
    should_fallback_from_responses_message, should_retry_without_stream_message,
};
use crate::openai::{StreamOutput, StreamRenderer};
use async_openai::types::responses::{
    AssistantRole, OutputContent, OutputItem, OutputMessage, OutputMessageContent, OutputStatus,
    OutputTextContent, ResponseContentPartDoneEvent, ResponseOutputItemDoneEvent,
    ResponseStreamEvent, ResponseTextDeltaEvent, ResponseTextDoneEvent,
};

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
fn deduplicates_delta_done_and_output_item_events() {
    let mut renderer = StreamRenderer::new(StreamOutput::None);
    let mut accumulator = ResponseTextAccumulator::default();

    for event in [
        ResponseStreamEvent::ResponseOutputTextDelta(ResponseTextDeltaEvent {
            sequence_number: 1,
            item_id: "msg_1".to_string(),
            output_index: 0,
            content_index: 0,
            delta: "feat: add parser".to_string(),
            logprobs: None,
        }),
        ResponseStreamEvent::ResponseOutputTextDone(ResponseTextDoneEvent {
            sequence_number: 2,
            item_id: "msg_1".to_string(),
            output_index: 0,
            content_index: 0,
            text: "feat: add parser".to_string(),
            logprobs: None,
        }),
        ResponseStreamEvent::ResponseContentPartDone(ResponseContentPartDoneEvent {
            sequence_number: 3,
            item_id: "msg_1".to_string(),
            output_index: 0,
            content_index: 0,
            part: OutputContent::OutputText(output_text("feat: add parser")),
        }),
        ResponseStreamEvent::ResponseOutputItemDone(ResponseOutputItemDoneEvent {
            sequence_number: 4,
            output_index: 0,
            item: OutputItem::Message(output_message(
                "msg_1",
                vec![OutputMessageContent::OutputText(output_text("feat: add parser"))],
            )),
        }),
    ] {
        let result =
            append_response_stream_event_text(event, &mut renderer, &mut accumulator, false);
        assert_eq!(result.unwrap(), None);
    }

    assert_eq!(accumulator.content(), "feat: add parser");
}

#[test]
fn backfills_done_only_stream_parts_once_in_order() {
    let mut renderer = StreamRenderer::new(StreamOutput::None);
    let mut accumulator = ResponseTextAccumulator::default();

    for event in [
        ResponseStreamEvent::ResponseContentPartDone(ResponseContentPartDoneEvent {
            sequence_number: 1,
            item_id: "msg_2".to_string(),
            output_index: 0,
            content_index: 0,
            part: OutputContent::OutputText(output_text("refactor: rewrite provider path\n\n")),
        }),
        ResponseStreamEvent::ResponseContentPartDone(ResponseContentPartDoneEvent {
            sequence_number: 2,
            item_id: "msg_2".to_string(),
            output_index: 0,
            content_index: 1,
            part: OutputContent::OutputText(output_text(
                "Prefer responses first and retry chat as fallback.",
            )),
        }),
        ResponseStreamEvent::ResponseOutputItemDone(ResponseOutputItemDoneEvent {
            sequence_number: 3,
            output_index: 0,
            item: OutputItem::Message(output_message(
                "msg_2",
                vec![
                    OutputMessageContent::OutputText(output_text(
                        "refactor: rewrite provider path\n\n",
                    )),
                    OutputMessageContent::OutputText(output_text(
                        "Prefer responses first and retry chat as fallback.",
                    )),
                ],
            )),
        }),
    ] {
        let result =
            append_response_stream_event_text(event, &mut renderer, &mut accumulator, false);
        assert_eq!(result.unwrap(), None);
    }

    assert_eq!(
        accumulator.content(),
        "refactor: rewrite provider path\n\nPrefer responses first and retry chat as fallback."
    );
}

fn output_text(text: &str) -> OutputTextContent {
    OutputTextContent {
        annotations: Vec::new(),
        logprobs: None,
        text: text.to_string(),
    }
}

fn output_message(id: &str, content: Vec<OutputMessageContent>) -> OutputMessage {
    OutputMessage {
        content,
        id: id.to_string(),
        role: AssistantRole::Assistant,
        phase: None,
        status: OutputStatus::Completed,
    }
}
