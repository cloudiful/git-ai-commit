use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct ChatCompletionResponse {
    pub(super) choices: Vec<Choice>,
    pub(super) error: Option<ProviderError>,
}

#[derive(Deserialize)]
pub(super) struct Choice {
    pub(super) message: AssistantMessage,
}

#[derive(Deserialize)]
pub(super) struct AssistantMessage {
    pub(super) content: String,
}

#[derive(Deserialize)]
pub(super) struct ProviderError {
    pub(super) message: String,
}

#[derive(Deserialize)]
pub(super) struct ResponsesApiResponse {
    #[serde(default)]
    pub(super) output_text: Option<String>,
    #[serde(default)]
    pub(super) output: Vec<ResponseOutputItem>,
    pub(super) error: Option<ProviderError>,
}

#[derive(Deserialize, Default)]
pub(super) struct ResponseOutputItem {
    #[serde(default)]
    pub(super) content: Vec<ResponseContentPart>,
}

#[derive(Deserialize, Default)]
pub(super) struct ResponseContentPart {
    #[serde(default)]
    pub(super) text: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ResponsesStreamEvent {
    #[serde(rename = "type", default)]
    pub(super) event_type: String,
    #[serde(default)]
    pub(super) delta: Option<String>,
    #[serde(default)]
    pub(super) text: Option<String>,
    #[serde(default)]
    pub(super) item: Option<ResponseOutputItem>,
    #[serde(default)]
    pub(super) part: Option<ResponseContentPart>,
    #[serde(default)]
    pub(super) error: Option<ProviderError>,
    #[serde(default)]
    pub(super) message: Option<String>,
}

#[derive(Deserialize, Default)]
pub(super) struct ChatCompletionChunk {
    #[serde(default)]
    pub(super) choices: Vec<ChunkChoice>,
    pub(super) error: Option<ProviderError>,
}

#[derive(Deserialize, Default)]
pub(super) struct ChunkChoice {
    #[serde(default)]
    pub(super) delta: ChunkDelta,
}

#[derive(Deserialize, Default)]
pub(super) struct ChunkDelta {
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) role: Option<String>,
}
