use crate::config::{Config, DEFAULT_MAX_DIFF_TOKENS, MAX_AUTO_DIFF_TOKENS};
use crate::provider_common::{new_http_client, provider_debug_enabled, truncate_debug_body};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::{apply_auth, models_url};

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

fn openrouter_context_cache() -> &'static Mutex<HashMap<(String, String), usize>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), usize>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn resolve_model_context_config(cfg: &Config, debug_provider: bool) -> Config {
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
        apply_auto_diff_token_limit(&mut resolved, value);
        return resolved;
    }

    match fetch_openrouter_model_context_tokens(cfg, debug_provider) {
        Ok(Some(value)) => {
            if let Ok(mut cache) = openrouter_context_cache().lock() {
                cache.insert(cache_key, value);
            }
            let mut resolved = cfg.clone();
            resolved.model_context_tokens = Some(value);
            apply_auto_diff_token_limit(&mut resolved, value);
            resolved
        }
        Ok(None) | Err(_) => cfg.clone(),
    }
}

pub(crate) fn detect_model_context_tokens(
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
    let debug_enabled = provider_debug_enabled(debug_provider);
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

fn apply_auto_diff_token_limit(cfg: &mut Config, model_context_tokens: usize) {
    if cfg.max_diff_tokens_explicit {
        return;
    }

    let suggested = (model_context_tokens / 4).clamp(DEFAULT_MAX_DIFF_TOKENS, MAX_AUTO_DIFF_TOKENS);
    cfg.max_diff_tokens = Some(suggested);
}

#[cfg(test)]
mod tests {
    use super::{
        apply_auto_diff_token_limit, detect_model_context_tokens,
        fetch_openrouter_model_context_tokens, resolve_model_context_config,
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
    fn auto_raises_default_max_diff_tokens_up_to_cap() {
        let mut cfg = sample_config(
            "https://openrouter.ai/api/v1",
            "deepseek/deepseek-v4-flash",
            None,
        );

        apply_auto_diff_token_limit(&mut cfg, 1_048_576);

        assert_eq!(cfg.max_diff_tokens, Some(64_000));
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

    #[test]
    fn preserves_explicit_max_diff_tokens() {
        let mut cfg = sample_config(
            "https://openrouter.ai/api/v1",
            "deepseek/deepseek-v4-flash",
            Some(1_048_576),
        );
        cfg.max_diff_tokens = Some(20_000);
        cfg.max_diff_tokens_explicit = true;

        let resolved = resolve_model_context_config(&cfg, false);

        assert_eq!(resolved.max_diff_tokens, Some(20_000));
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
            max_diff_tokens_explicit: false,
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
