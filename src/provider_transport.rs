use crate::config::Config;
use crate::git::RepoContext;
use crate::openai::{GenerationMetrics, StreamOutput};

pub(crate) trait CommitMessageTransport {
    fn generate(
        &self,
        cfg: &Config,
        repo_ctx: &RepoContext,
        stream_output: StreamOutput,
        debug_provider: bool,
    ) -> Result<(String, GenerationMetrics), String>;
}

pub(crate) struct OpenAiTransport;
pub(crate) struct AnthropicTransport;
