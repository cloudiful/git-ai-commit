use crate::config::{Config, Provider, load_partial_config, missing_required_config_keys};
use crate::git::run_git;
use crate::openai::{apply_auth, detect_model_context_tokens, models_url};
use crate::provider_common::new_http_client;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Default)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelSummary>,
    error: Option<DoctorProviderError>,
}

#[derive(Deserialize, Default)]
struct ModelSummary {
    id: String,
}

#[derive(Deserialize)]
struct DoctorProviderError {
    message: String,
}

pub(super) fn run_doctor(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err("doctor does not accept arguments".to_string());
    }

    match load_partial_config() {
        Ok(cfg) => {
            let missing = missing_required_config_keys(&cfg);
            if missing.is_empty() {
                println!("config: ready");
            } else {
                println!("config: not ready (missing {})", missing.join(", "));
            }
            println!("provider: {}", cfg.provider.as_config_value());
            println!("transport: {}", transport_label(&cfg));
            println!("api base: {}", display_doctor_value(&cfg.api_base));
            println!("model: {}", display_doctor_value(&cfg.model));
            println!(
                "model context tokens: {}",
                display_model_context_tokens(&cfg)
            );
            println!("auth: {}", cfg.auth_mode_description());

            if cfg.should_auto_detect_model_context_tokens() {
                match detect_model_context_tokens(&cfg, false) {
                    Ok(Some(value)) => {
                        println!("model context tokens (auto): {value}");
                    }
                    Ok(None) => {
                        println!("model context tokens (auto): unavailable");
                    }
                    Err(err) => {
                        println!("model context tokens (auto): lookup failed ({err})");
                    }
                }
            }

            if cfg.provider == Provider::Ollama {
                for line in doctor_ollama_lines(&cfg) {
                    println!("{line}");
                }
            }
        }
        Err(err) => println!("config: not ready ({err})"),
    }

    match run_git(None::<&Path>, ["rev-parse", "--show-toplevel"]) {
        Ok(repo_root) => println!("repo: {}", repo_root.trim()),
        Err(err) => println!("repo: not detected ({err})"),
    }

    Ok(())
}

fn display_doctor_value(value: &str) -> &str {
    if value.trim().is_empty() {
        "(unset)"
    } else {
        value
    }
}

fn display_model_context_tokens(cfg: &Config) -> String {
    cfg.model_context_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| "(unset)".to_string())
}

fn transport_label(cfg: &Config) -> &'static str {
    if cfg.should_use_anthropic_transport() {
        if cfg.provider == Provider::AnthropicCompatible {
            "anthropic-compatible"
        } else {
            "anthropic-compatible (auto)"
        }
    } else {
        "openai-compatible"
    }
}

fn doctor_ollama_lines(cfg: &Config) -> Vec<String> {
    if cfg.is_ollama_cloud() && cfg.api_key.trim().is_empty() {
        return vec!["ollama endpoint: auth missing for ollama cloud".to_string()];
    }

    match probe_ollama_endpoint(cfg) {
        Ok(probe) => {
            let mut lines = vec![format!(
                "ollama endpoint: reachable ({} model(s) visible)",
                probe.visible_model_count
            )];
            if cfg.model.trim().is_empty() {
                lines.push("ollama model: not checked (model unset)".to_string());
            } else if probe.model_found {
                lines.push(format!("ollama model: found ({})", cfg.model));
            } else {
                lines.push(format!(
                    "ollama model: missing ({}). Pull it first with: ollama pull {}",
                    cfg.model, cfg.model
                ));
            }
            lines
        }
        Err(err) => vec![format!("ollama endpoint: {err}")],
    }
}

struct OllamaProbe {
    visible_model_count: usize,
    model_found: bool,
}

fn probe_ollama_endpoint(cfg: &Config) -> Result<OllamaProbe, String> {
    let client = new_http_client(cfg)?;
    let response = apply_auth(client.get(models_url(&cfg.api_base)), cfg)
        .send()
        .map_err(|err| format!("probe failed: {err}"))?;
    let status = response.status().as_u16();
    let body = response.text().map_err(|err| err.to_string())?;

    if status >= 400 {
        if matches!(status, 404 | 405 | 415 | 501) {
            return Err(
                "incompatible endpoint (expected OpenAI-compatible /v1/models)".to_string(),
            );
        }

        if let Ok(parsed) = serde_json::from_str::<ModelsResponse>(&body)
            && let Some(error) = parsed.error
            && !error.message.trim().is_empty()
        {
            return Err(format!("probe failed: {}", error.message));
        }

        return Err(format!("probe failed with status {status}"));
    }

    let parsed: ModelsResponse = serde_json::from_str(&body)
        .map_err(|err| format!("incompatible endpoint (invalid /v1/models response: {err})"))?;
    let model_found = parsed.data.iter().any(|model| model.id == cfg.model);

    Ok(OllamaProbe {
        visible_model_count: parsed.data.len(),
        model_found,
    })
}

#[cfg(test)]
mod tests {
    use super::doctor_ollama_lines;
    use crate::config::{Config, Provider};
    use crate::openai::{apply_auth, models_url};
    use crate::provider_common::new_http_client;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn omits_bearer_auth_for_local_ollama_without_key() {
        let (base, requests, handle) = spawn_http_once(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":"llama3.2"}]}"#,
        );
        let cfg = sample_config(Provider::Ollama, &base, "", "llama3.2");
        let client = new_http_client(&cfg).expect("client");

        let response = apply_auth(client.get(models_url(&cfg.api_base)), &cfg)
            .send()
            .expect("request");
        assert!(response.status().is_success());

        let request = requests.recv().expect("captured request");
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        handle.join().expect("server thread");
    }

    #[test]
    fn includes_bearer_auth_when_api_key_is_configured() {
        let (base, requests, handle) = spawn_http_once(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":"gpt-oss:20b"}]}"#,
        );
        let cfg = sample_config(Provider::Ollama, &base, "secret-token", "gpt-oss:20b");
        let client = new_http_client(&cfg).expect("client");

        let response = apply_auth(client.get(models_url(&cfg.api_base)), &cfg)
            .send()
            .expect("request");
        assert!(response.status().is_success());

        let request = requests.recv().expect("captured request");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer secret-token")
        );
        handle.join().expect("server thread");
    }

    #[test]
    fn doctor_reports_ollama_local_success() {
        let (base, _requests, handle) = spawn_http_once(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":"llama3.2"},{"id":"qwen3:8b"}]}"#,
        );
        let cfg = sample_config(Provider::Ollama, &base, "", "llama3.2");

        let lines = doctor_ollama_lines(&cfg);

        assert!(
            lines
                .iter()
                .any(|line| line.contains("reachable (2 model(s) visible)"))
        );
        assert!(lines.iter().any(|line| line.contains("found (llama3.2)")));
        handle.join().expect("server thread");
    }

    #[test]
    fn doctor_reports_missing_ollama_model() {
        let (base, _requests, handle) = spawn_http_once(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":"qwen3:8b"}]}"#,
        );
        let cfg = sample_config(Provider::Ollama, &base, "", "llama3.2");

        let lines = doctor_ollama_lines(&cfg);

        assert!(
            lines
                .iter()
                .any(|line| line.contains("ollama model: missing (llama3.2)"))
        );
        handle.join().expect("server thread");
    }

    #[test]
    fn doctor_reports_cloud_auth_missing_before_probe() {
        let cfg = sample_config(Provider::Ollama, "https://ollama.com/v1", "", "gpt-oss:20b");

        let lines = doctor_ollama_lines(&cfg);

        assert_eq!(
            lines,
            vec!["ollama endpoint: auth missing for ollama cloud"]
        );
    }

    #[test]
    fn doctor_reports_incompatible_ollama_endpoint() {
        let (base, _requests, handle) = spawn_http_once("404 Not Found", "text/plain", "missing");
        let cfg = sample_config(Provider::Ollama, &base, "", "llama3.2");

        let lines = doctor_ollama_lines(&cfg);

        assert!(
            lines
                .iter()
                .any(|line| line.contains("incompatible endpoint"))
        );
        handle.join().expect("server thread");
    }

    fn sample_config(provider: Provider, api_base: &str, api_key: &str, model: &str) -> Config {
        Config {
            provider,
            api_base: api_base.to_string(),
            api_key: api_key.to_string(),
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
            model_context_tokens: None,
        }
    }

    fn spawn_http_once(
        status: &str,
        content_type: &str,
        body: &str,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("listener addr");
        let (tx, rx) = mpsc::channel();
        let status = status.to_string();
        let content_type = content_type.to_string();
        let body = body.to_string();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 8192];
            let bytes = stream.read(&mut buffer).expect("read request");
            tx.send(String::from_utf8_lossy(&buffer[..bytes]).to_string())
                .expect("send request");

            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });

        (format!("http://{addr}"), rx, handle)
    }
}
