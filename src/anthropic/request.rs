use serde::Serialize;

pub(crate) const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Serialize)]
pub(crate) struct MessagesRequest {
    pub(crate) model: String,
    pub(crate) system: String,
    pub(crate) messages: Vec<Message>,
    pub(crate) max_tokens: u32,
    pub(crate) temperature: f64,
    pub(crate) stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thinking: Option<ThinkingConfig>,
}

#[derive(Serialize)]
pub(crate) struct Message {
    pub(crate) role: &'static str,
    pub(crate) content: String,
}

#[derive(Serialize)]
pub(crate) struct ThinkingConfig {
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
}

pub(crate) fn messages_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else if base.ends_with("/messages") {
        base.to_string()
    } else {
        format!("{base}/v1/messages")
    }
}

pub(crate) fn disabled_thinking(base: &str) -> Option<ThinkingConfig> {
    let base = base.trim().to_ascii_lowercase();
    if base.contains("deepseek.com/anthropic") {
        Some(ThinkingConfig { kind: "disabled" })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{disabled_thinking, messages_url};

    #[test]
    fn appends_messages_endpoint() {
        assert_eq!(
            messages_url("https://api.deepseek.com/anthropic"),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
        assert_eq!(
            messages_url("https://api.deepseek.com/anthropic/v1"),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
    }

    #[test]
    fn enables_disabled_thinking_for_deepseek_anthropic() {
        assert!(disabled_thinking("https://api.deepseek.com/anthropic").is_some());
        assert!(disabled_thinking("https://api.deepseek.com/anthropic/v1").is_some());
        assert!(disabled_thinking("https://api.anthropic.com").is_none());
    }
}
