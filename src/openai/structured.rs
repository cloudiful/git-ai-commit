use crate::config::Config;
use crate::message::format_commit_message;
use crate::provider_common::provider_debug_enabled;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use super::request::{
    ChatCompletionRequest, ChatMessage, ChatResponseFormat, ChatResponseFormatJsonSchema,
    chat_completions_url,
};
use super::response::parse_json_chat_completion_content;
use super::{MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, apply_auth};

#[derive(Deserialize)]
struct StructuredCommitMessage {
    subject: String,
    #[serde(default)]
    body: Vec<String>,
}

#[derive(Clone, Copy)]
enum StructuredMode {
    JsonSchema,
    JsonObject,
}

pub(crate) fn generate_structured_message_via_chat_completions(
    cfg: &Config,
    client: &Client,
    prompt: &str,
    debug_provider: bool,
) -> Result<String, String> {
    match generate_structured_message_via_chat_completions_mode(
        cfg,
        client,
        prompt,
        debug_provider,
        StructuredMode::JsonSchema,
    ) {
        Ok(message) => Ok(message),
        Err(err) if supports_json_object_only(&err) => {
            if provider_debug_enabled(debug_provider) {
                eprintln!(
                    "git-ai-commit: provider debug: structured mode json_schema unsupported, retrying with json_object"
                );
            }
            generate_structured_message_via_chat_completions_mode(
                cfg,
                client,
                prompt,
                debug_provider,
                StructuredMode::JsonObject,
            )
        }
        Err(err) => Err(err),
    }
}

fn generate_structured_message_via_chat_completions_mode(
    cfg: &Config,
    client: &Client,
    prompt: &str,
    debug_provider: bool,
    mode: StructuredMode,
) -> Result<String, String> {
    let debug_enabled = provider_debug_enabled(debug_provider);
    let request = ChatCompletionRequest {
        model: cfg.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system",
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user",
                content: prompt.to_string(),
            },
        ],
        temperature: 0.1,
        max_tokens: MAX_OUTPUT_TOKENS as u32,
        stream: false,
        response_format: Some(structured_response_format(mode)),
    };

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream=false response_format={}",
            chat_completions_url(&cfg.api_base),
            cfg.model,
            match mode {
                StructuredMode::JsonSchema => "json_schema",
                StructuredMode::JsonObject => "json_object",
            }
        );
    }

    let response = apply_auth(client.post(chat_completions_url(&cfg.api_base)), cfg)
        .json(&request)
        .send()
        .map_err(|err| err.to_string())?;

    let status_code = response.status().as_u16();
    let body = response.text().map_err(|err| err.to_string())?;

    let content = parse_json_chat_completion_content(status_code, &body)?;
    let structured: StructuredCommitMessage = serde_json::from_str(&content)
        .map_err(|err| format!("invalid structured commit message payload: {err}"))?;
    format_commit_message(&structured.subject, &structured.body)
}

fn structured_response_format(mode: StructuredMode) -> ChatResponseFormat {
    match mode {
        StructuredMode::JsonSchema => ChatResponseFormat {
            format_type: "json_schema",
            json_schema: Some(ChatResponseFormatJsonSchema {
                name: "git_commit_message",
                strict: true,
                schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "subject": { "type": "string" },
                        "body": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    },
                    "required": ["subject", "body"]
                }),
            }),
        },
        StructuredMode::JsonObject => ChatResponseFormat {
            format_type: "json_object",
            json_schema: None,
        },
    }
}

fn supports_json_object_only(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    lowered.contains("does not support 'json_schema'")
        && lowered.contains("supported formats: json_object")
}
