use serde_json::Value;

pub(super) fn extract_response_text_value(response: &Value) -> Option<String> {
    match response {
        Value::Object(_) => response
            .get("output_text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                response
                    .get("output")
                    .and_then(Value::as_array)
                    .and_then(|items| extract_output_items_text(items))
            })
            .or_else(|| extract_message_item_text(response)),
        Value::Array(items) => extract_output_items_text(items),
        _ => None,
    }
}

fn extract_output_items_text(items: &[Value]) -> Option<String> {
    let mut out = String::new();

    for item in items {
        if let Some(text) = extract_message_item_text(item) {
            out.push_str(&text);
        }
    }

    if out.is_empty() { None } else { Some(out) }
}

fn extract_message_item_text(item: &Value) -> Option<String> {
    if item.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }

    let parts = item.get("content").and_then(Value::as_array)?;
    let mut out = String::new();

    for part in parts {
        if part.get("type").and_then(Value::as_str) != Some("output_text") {
            continue;
        }
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            out.push_str(text);
        }
    }

    if out.is_empty() { None } else { Some(out) }
}
