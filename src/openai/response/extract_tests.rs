use crate::openai::response::extract::extract_response_text_value;
use serde_json::json;

#[test]
fn extracts_response_text_from_top_level_output_item_array() {
    let payload = json!([
        {
            "type": "reasoning",
            "id": "06973324f9be83fcfcd64eeaad409696_rs",
            "status": "completed",
            "content": [
                {
                    "type": "reasoning_text",
                    "text": "The user wants me to generate a commit message."
                }
            ]
        },
        {
            "type": "message",
            "id": "06973324f9be83fcfcd64eeaad409696_msg",
            "role": "assistant",
            "status": "completed",
            "content": [
                {
                    "type": "output_text",
                    "text": "refactor(rust-lsp): delegate workspace setup to stock-script-service"
                }
            ]
        }
    ]);

    assert_eq!(
        extract_response_text_value(&payload).as_deref(),
        Some("refactor(rust-lsp): delegate workspace setup to stock-script-service")
    );
}
