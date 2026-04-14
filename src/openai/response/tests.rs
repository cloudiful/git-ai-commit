use super::{
    collect_streaming_chat_completion, parse_json_chat_completion, parse_responses_api_response,
    should_fallback_from_responses,
};
use crate::openai::StreamOutput;
use crate::openai::stream::StreamRenderer;
use std::io::Cursor;

#[test]
fn parses_json_response() {
    let body = r#"{"choices":[{"message":{"content":"feat: add parser"}}]}"#;
    let message = parse_json_chat_completion(200, body).unwrap();
    assert_eq!(message, "feat: add parser");
}

#[test]
fn parses_responses_output_text() {
    let body = r#"{"output_text":"feat: add parser"}"#;
    let message = parse_responses_api_response(200, body).unwrap();
    assert_eq!(message, "feat: add parser");
}

#[test]
fn parses_responses_output_content() {
    let body = r#"{"output":[{"content":[{"text":"feat: add parser"}]}]}"#;
    let message = parse_responses_api_response(200, body).unwrap();
    assert_eq!(message, "feat: add parser");
}

#[test]
fn streaming_chat_completion_handles_role_and_empty_delta_chunks() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"feat:\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\" add parser\"}}]}\n\n",
        "data: [DONE]\n"
    );
    let mut renderer = StreamRenderer::new(StreamOutput::None);

    let message = collect_streaming_chat_completion(200, Cursor::new(body), &mut renderer).unwrap();

    assert_eq!(message, "feat: add parser");
}

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
    assert!(!should_fallback_from_responses(401, "invalid api key",));
}
