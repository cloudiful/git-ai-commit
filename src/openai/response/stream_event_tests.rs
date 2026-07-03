use crate::openai::{StreamOutput, StreamRenderer};
use crate::openai::response::{
    ResponseTextAccumulator, append_response_stream_event_text,
};
use serde_json::json;

#[test]
fn backfills_output_text_from_content_part_added_events() {
    let mut renderer = StreamRenderer::new(StreamOutput::None);
    let mut accumulator = ResponseTextAccumulator::default();

    for event in [
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "type": "message",
                "id": "msg_1",
                "content": []
            }
        }),
        json!({
            "type": "response.content_part.added",
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": "feat: add parser"
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
fn output_item_added_with_message_content_is_extracted_once() {
    let mut renderer = StreamRenderer::new(StreamOutput::None);
    let mut accumulator = ResponseTextAccumulator::default();

    let event = json!({
        "type": "response.output_item.added",
        "output_index": 0,
        "item": {
            "type": "message",
            "id": "msg_2",
            "content": [{
                "type": "output_text",
                "text": "refactor: rewrite provider path"
            }]
        }
    });

    let result = append_response_stream_event_text(&event, &mut renderer, &mut accumulator, false);
    assert_eq!(result.unwrap(), None);
    assert_eq!(accumulator.content(), "refactor: rewrite provider path");
}
