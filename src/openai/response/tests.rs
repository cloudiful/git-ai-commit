use super::{should_fallback_from_responses, should_fallback_from_responses_message};

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
}
