mod request;
mod response;
mod stream;

use crate::config::Config;
use crate::message::validate_message;
use reqwest::blocking::Client;
use reqwest::blocking::RequestBuilder;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use self::request::{
    ChatCompletionRequest, ChatMessage, ResponseInputMessage, ResponsesRequest, build_prompt,
    chat_completions_url, responses_url,
};
use self::response::{
    parse_chat_completion_response, parse_responses_response, should_fallback_from_responses,
};
use self::stream::StreamRenderer;

pub(crate) use self::request::models_url;
pub(crate) use self::request::{MAX_OUTPUT_TOKENS, SYSTEM_PROMPT, build_prompt_scaffold};

#[derive(Clone, Copy, Debug, Default)]
pub struct GenerationMetrics {
    pub api_duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamOutput {
    None,
    Stdout,
    Stderr,
}

#[derive(Deserialize, Default)]
struct ModelsCatalogResponse {
    #[serde(default)]
    data: Vec<ModelCatalogEntry>,
}

#[derive(Deserialize, Default)]
struct ModelCatalogEntry {
    id: String,
    #[serde(default)]
    context_length: Option<usize>,
    #[serde(default)]
    top_provider: Option<ModelTopProvider>,
}

#[derive(Deserialize, Default)]
struct ModelTopProvider {
    #[serde(default)]
    context_length: Option<usize>,
}

fn env_debug_provider_enabled() -> bool {
    matches!(
        std::env::var("GIT_AI_COMMIT_DEBUG_PROVIDER")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn openrouter_context_cache() -> &'static Mutex<HashMap<(String, String), usize>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), usize>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn resolve_model_context_config(cfg: &Config, debug_provider: bool) -> Config {
    if !cfg.should_auto_detect_model_context_tokens() {
        return cfg.clone();
    }

    let cache_key = (cfg.api_base.clone(), cfg.model.clone());
    if let Some(value) = openrouter_context_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&cache_key).copied())
    {
        let mut resolved = cfg.clone();
        resolved.model_context_tokens = Some(value);
        return resolved;
    }

    match fetch_openrouter_model_context_tokens(cfg, debug_provider) {
        Ok(Some(value)) => {
            if let Ok(mut cache) = openrouter_context_cache().lock() {
                cache.insert(cache_key, value);
            }
            let mut resolved = cfg.clone();
            resolved.model_context_tokens = Some(value);
            resolved
        }
        Ok(None) | Err(_) => cfg.clone(),
    }
}

pub fn detect_model_context_tokens(
    cfg: &Config,
    debug_provider: bool,
) -> Result<Option<usize>, String> {
    if !cfg.should_auto_detect_model_context_tokens() {
        return Ok(cfg.model_context_tokens);
    }

    let cache_key = (cfg.api_base.clone(), cfg.model.clone());
    if let Some(value) = openrouter_context_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&cache_key).copied())
    {
        return Ok(Some(value));
    }

    let detected = fetch_openrouter_model_context_tokens(cfg, debug_provider)?;
    if let Some(value) = detected
        && let Ok(mut cache) = openrouter_context_cache().lock()
    {
        cache.insert(cache_key, value);
    }
    Ok(detected)
}

fn fetch_openrouter_model_context_tokens(
    cfg: &Config,
    debug_provider: bool,
) -> Result<Option<usize>, String> {
    let debug_enabled = debug_provider || env_debug_provider_enabled();
    let client = new_http_client(cfg)?;
    let url = models_url(&cfg.api_base);

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: GET {} for model metadata ({})",
            url, cfg.model
        );
    }

    let response = apply_auth(client.get(&url), cfg)
        .send()
        .map_err(|err| format!("openrouter models lookup failed: {err}"))?;
    let status_code = response.status().as_u16();
    let body = response
        .text()
        .map_err(|err| format!("openrouter models lookup failed: {err}"))?;

    if status_code >= 400 {
        if debug_enabled {
            eprintln!(
                "git-ai-commit: provider debug: models lookup failed with status {} body={}",
                status_code,
                truncate_debug_body(&body)
            );
        }
        return Err(format!(
            "openrouter models lookup failed with status {status_code}"
        ));
    }

    let parsed: ModelsCatalogResponse = serde_json::from_str(&body)
        .map_err(|err| format!("invalid OpenRouter models payload: {err}"))?;

    let detected = parsed
        .data
        .into_iter()
        .find(|entry| entry.id == cfg.model)
        .and_then(|entry| {
            entry
                .top_provider
                .and_then(|p| p.context_length)
                .or(entry.context_length)
        });

    if debug_enabled {
        match detected {
            Some(value) => eprintln!(
                "git-ai-commit: provider debug: auto-detected context_length={} for {}",
                value, cfg.model
            ),
            None => eprintln!(
                "git-ai-commit: provider debug: model {} not found in OpenRouter models catalog",
                cfg.model
            ),
        }
    }

    Ok(detected)
}

fn truncate_debug_body(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(400).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::{
        detect_model_context_tokens, fetch_openrouter_model_context_tokens,
        resolve_model_context_config,
    };
    use crate::config::{Config, Provider};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn fetches_openrouter_model_context_tokens() {
        let (base, _requests, handle) = spawn_http_once(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":"google/gemma-4-31b-it:free","context_length":32768,"top_provider":{"context_length":65536}}]}"#,
        );
        let cfg = sample_config(&base, "google/gemma-4-31b-it:free", None);

        let detected =
            fetch_openrouter_model_context_tokens(&cfg, false).expect("context token lookup");

        assert_eq!(detected, Some(65536));
        handle.join().expect("server thread");
    }

    #[test]
    fn preserves_explicit_model_context_tokens() {
        let cfg = sample_config(
            "https://openrouter.ai/api/v1",
            "google/gemma-4-31b-it:free",
            Some(12345),
        );

        let resolved = resolve_model_context_config(&cfg, false);

        assert_eq!(resolved.model_context_tokens, Some(12345));
    }

    #[test]
    fn returns_none_when_model_missing_from_openrouter_catalog() {
        let (base, _requests, handle) = spawn_http_once(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":"other/model","context_length":8192}]}"#,
        );
        let cfg = sample_config(&base, "google/gemma-4-31b-it:free", None);

        let detected = detect_model_context_tokens(&cfg, false).expect("lookup result");

        assert_eq!(detected, None);
        handle.join().expect("server thread");
    }

    fn sample_config(api_base: &str, model: &str, model_context_tokens: Option<usize>) -> Config {
        Config {
            provider: Provider::OpenAiCompatible,
            api_base: api_base.to_string(),
            api_key: "secret-token".to_string(),
            model: model.to_string(),
            confirm_commit: true,
            open_editor: false,
            redact_secrets: true,
            show_timing: true,
            use_env_proxy: false,
            timeout: Duration::from_secs(5),
            max_diff_bytes: 60_000,
            max_diff_tokens: Some(16_000),
            model_context_tokens,
        }
    }

    fn spawn_http_once(
        status: &str,
        content_type: &str,
        body: &str,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        listener
            .set_nonblocking(false)
            .expect("listener blocking mode");
        let address = listener.local_addr().expect("listener addr");
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut buffer = [0u8; 4096];
            let bytes_read = stream.read(&mut buffer).expect("read request");
            tx.send(String::from_utf8_lossy(&buffer[..bytes_read]).into_owned())
                .expect("send request");
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });

        (format!("http://{address}/api/v1"), rx, handle)
    }
}

pub fn generate_message_with_stream_output(
    cfg: &Config,
    repo_ctx: &crate::git::RepoContext,
    stream_output: StreamOutput,
    debug_provider: bool,
) -> Result<(String, GenerationMetrics), String> {
    let client = new_http_client(cfg)?;
    let prompt = build_prompt(repo_ctx);
    let started = Instant::now();
    let mut renderer = StreamRenderer::new(stream_output);
    let message = match generate_message_via_responses(
        cfg,
        &client,
        &prompt,
        &mut renderer,
        debug_provider,
    ) {
        Ok(message) => message,
        Err(err) if err.should_fallback => {
            renderer.reset();
            generate_message_via_chat_completions(
                cfg,
                &client,
                &prompt,
                &mut renderer,
                debug_provider,
            )
            .map_err(|fallback_err| {
                format!("{fallback_err} (responses fallback: {})", err.message)
            })?
        }
        Err(err) => return Err(err.message),
    };
    renderer.finish().map_err(|err| err.to_string())?;
    let metrics = GenerationMetrics {
        api_duration: started.elapsed(),
    };
    validate_message(&message)?;
    Ok((message, metrics))
}

#[derive(Debug)]
struct ApiAttemptError {
    message: String,
    should_fallback: bool,
}

pub(crate) fn new_http_client(cfg: &Config) -> Result<Client, String> {
    let mut builder = Client::builder().timeout(cfg.timeout);
    if !cfg.use_env_proxy {
        builder = builder.no_proxy();
    }
    builder.build().map_err(|err| err.to_string())
}

pub(crate) fn apply_auth(builder: RequestBuilder, cfg: &Config) -> RequestBuilder {
    if cfg.should_send_bearer_auth() {
        builder.bearer_auth(&cfg.api_key)
    } else {
        builder
    }
}

fn generate_message_via_responses(
    cfg: &Config,
    client: &Client,
    prompt: &str,
    renderer: &mut StreamRenderer,
    debug_provider: bool,
) -> Result<String, ApiAttemptError> {
    let debug_enabled = debug_provider || env_debug_provider_enabled();
    let request = ResponsesRequest {
        model: cfg.model.clone(),
        instructions: SYSTEM_PROMPT.to_string(),
        input: vec![ResponseInputMessage {
            role: "user",
            content: prompt.to_string(),
        }],
        temperature: 0.2,
        max_output_tokens: MAX_OUTPUT_TOKENS as u32,
        stream: renderer.enabled(),
    };

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream={}",
            responses_url(&cfg.api_base),
            cfg.model,
            renderer.enabled()
        );
    }

    let response = apply_auth(client.post(responses_url(&cfg.api_base)), cfg)
        .json(&request)
        .send()
        .map_err(|err| ApiAttemptError {
            message: err.to_string(),
            should_fallback: false,
        })?;

    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();

    parse_responses_response(
        status_code,
        &content_type,
        response,
        renderer,
        debug_enabled,
    )
    .map_err(|message| {
        let should_fallback = should_fallback_from_responses(status_code, &message);
        ApiAttemptError {
            message,
            should_fallback,
        }
    })
}

fn generate_message_via_chat_completions(
    cfg: &Config,
    client: &Client,
    prompt: &str,
    renderer: &mut StreamRenderer,
    debug_provider: bool,
) -> Result<String, String> {
    let debug_enabled = debug_provider || env_debug_provider_enabled();
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
        temperature: 0.2,
        max_tokens: MAX_OUTPUT_TOKENS as u32,
        stream: renderer.enabled(),
    };

    if debug_enabled {
        eprintln!(
            "git-ai-commit: provider debug: POST {} model={} stream={}",
            chat_completions_url(&cfg.api_base),
            cfg.model,
            renderer.enabled()
        );
    }

    let response = apply_auth(client.post(chat_completions_url(&cfg.api_base)), cfg)
        .json(&request)
        .send()
        .map_err(|err| err.to_string())?;

    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();

    parse_chat_completion_response(
        status_code,
        &content_type,
        response,
        renderer,
        debug_enabled,
    )
}
