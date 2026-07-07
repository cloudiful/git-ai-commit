use crate::config::Config;
use crate::git::{RepoContext, collect_repo_context};
use crate::openai::{
    GeneratedMessage, StreamOutput, generate_message_with_stream_output,
    resolve_model_context_config,
};
use crate::prompt::{is_interactive_session, load_config_for_interactive_use};
use std::io::IsTerminal;

pub(crate) struct PreparedGeneration {
    pub(crate) cfg: Config,
    pub(crate) repo_ctx: RepoContext,
    pub(crate) stream_output: StreamOutput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamOutputMode {
    InteractiveSession,
    StdoutTerminal,
}

pub(crate) async fn prepare_generation(
    debug_provider: bool,
    stream_output_mode: StreamOutputMode,
) -> Result<PreparedGeneration, String> {
    let cfg =
        resolve_model_context_config(&load_config_for_interactive_use()?, debug_provider).await;
    let repo_ctx = collect_repo_context(&cfg)?;

    Ok(PreparedGeneration {
        cfg,
        repo_ctx,
        stream_output: resolve_stream_output(stream_output_mode),
    })
}

pub(crate) async fn execute_generation(
    prepared: &PreparedGeneration,
    debug_provider: bool,
) -> Result<GeneratedMessage, String> {
    generate_message_with_stream_output(
        &prepared.cfg,
        &prepared.repo_ctx,
        prepared.stream_output,
        debug_provider,
    )
    .await
}

fn resolve_stream_output(mode: StreamOutputMode) -> StreamOutput {
    match mode {
        StreamOutputMode::InteractiveSession if is_interactive_session() => StreamOutput::Stdout,
        StreamOutputMode::StdoutTerminal if std::io::stdout().is_terminal() => StreamOutput::Stdout,
        _ => StreamOutput::None,
    }
}
