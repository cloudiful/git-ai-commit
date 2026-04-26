use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct MessagesResponse {
    #[serde(default)]
    pub(crate) content: Vec<ContentBlock>,
    pub(crate) error: Option<ProviderError>,
}

#[derive(Deserialize)]
pub(crate) struct ContentBlock {
    #[serde(rename = "type", default)]
    pub(crate) block_type: String,
    #[serde(default)]
    pub(crate) text: Option<String>,
    #[serde(default)]
    pub(crate) thinking: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ProviderError {
    pub(crate) message: String,
}

impl MessagesResponse {
    pub(crate) fn text_content(&self) -> String {
        self.content
            .iter()
            .filter(|part| part.block_type == "text")
            .filter_map(|part| part.text.as_deref())
            .collect::<String>()
    }

    pub(crate) fn block_types(&self) -> Vec<&str> {
        self.content
            .iter()
            .map(|part| part.block_type.as_str())
            .collect()
    }

    pub(crate) fn has_thinking(&self) -> bool {
        self.content.iter().any(|part| {
            part.block_type == "thinking"
                && part
                    .thinking
                    .as_deref()
                    .is_some_and(|thinking| !thinking.trim().is_empty())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::MessagesResponse;

    #[test]
    fn extracts_only_text_blocks() {
        let parsed: MessagesResponse = serde_json::from_str(
            r#"{
                "content": [
                    { "type": "thinking", "thinking": "internal" },
                    { "type": "text", "text": "refactor: split provider transport" }
                ]
            }"#,
        )
        .expect("expected valid response");

        assert_eq!(parsed.text_content(), "refactor: split provider transport");
        assert_eq!(parsed.block_types(), vec!["thinking", "text"]);
        assert!(parsed.has_thinking());
    }
}
