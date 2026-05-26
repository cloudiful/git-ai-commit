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

pub(super) async fn run_doctor(args: &[String]) -> Result<(), String> {
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
                match detect_model_context_tokens(&cfg, false).await {
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
                for line in doctor_ollama_lines(&cfg).await {
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

async fn doctor_ollama_lines(cfg: &Config) -> Vec<String> {
    if cfg.is_ollama_cloud() && cfg.api_key.trim().is_empty() {
        return vec!["ollama endpoint: auth missing for ollama cloud".to_string()];
    }

    match probe_ollama_endpoint(cfg).await {
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

#[derive(Debug)]
struct OllamaProbe {
    visible_model_count: usize,
    model_found: bool,
}

async fn probe_ollama_endpoint(cfg: &Config) -> Result<OllamaProbe, String> {
    let client = new_http_client(cfg)?;
    let response = apply_auth(client.get(models_url(&cfg.api_base)), cfg)
        .send()
        .await
        .map_err(|err| format!("probe failed: {err}"))?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|err| err.to_string())?;

    parse_ollama_probe_response(status, &body, cfg)
}

fn parse_ollama_probe_response(
    status: u16,
    body: &str,
    cfg: &Config,
) -> Result<OllamaProbe, String> {
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
    use reqwest::header::AUTHORIZATION;
    use std::time::Duration;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }

    #[test]
    fn omits_bearer_auth_for_local_ollama_without_key() {
        let cfg = sample_config(Provider::Ollama, "http://127.0.0.1:11434", "", "llama3.2");
        let client = new_http_client(&cfg).expect("client");
        let request = apply_auth(client.get(models_url(&cfg.api_base)), &cfg)
            .build()
            .expect("request");

        assert!(request.headers().get(AUTHORIZATION).is_none());
    }

    #[test]
    fn includes_bearer_auth_when_api_key_is_configured() {
        let cfg = sample_config(
            Provider::Ollama,
            "https://ollama.com/v1",
            "secret-token",
            "gpt-oss:20b",
        );
        let client = new_http_client(&cfg).expect("client");
        let request = apply_auth(client.get(models_url(&cfg.api_base)), &cfg)
            .build()
            .expect("request");

        assert_eq!(
            request
                .headers()
                .get(AUTHORIZATION)
                .expect("authorization header"),
            "Bearer secret-token"
        );
    }

    #[test]
    fn doctor_reports_ollama_local_success() {
        let cfg = sample_config(Provider::Ollama, "http://127.0.0.1:11434", "", "llama3.2");
        let probe = super::parse_ollama_probe_response(
            200,
            r#"{"data":[{"id":"llama3.2"},{"id":"qwen3:8b"}]}"#,
            &cfg,
        )
        .expect("probe");
        let lines = vec![
            format!(
                "ollama endpoint: reachable ({} model(s) visible)",
                probe.visible_model_count
            ),
            format!("ollama model: found ({})", cfg.model),
        ];

        assert!(
            lines
                .iter()
                .any(|line| line.contains("reachable (2 model(s) visible)"))
        );
        assert!(lines.iter().any(|line| line.contains("found (llama3.2)")));
    }

    #[test]
    fn doctor_reports_missing_ollama_model() {
        let cfg = sample_config(Provider::Ollama, "http://127.0.0.1:11434", "", "llama3.2");
        let probe = super::parse_ollama_probe_response(
            200,
            r#"{"data":[{"id":"qwen3:8b"}]}"#,
            &cfg,
        )
        .expect("probe");
        let lines = vec![
            format!(
                "ollama endpoint: reachable ({} model(s) visible)",
                probe.visible_model_count
            ),
            format!(
                "ollama model: missing ({}). Pull it first with: ollama pull {}",
                cfg.model, cfg.model
            ),
        ];

        assert!(
            lines
                .iter()
                .any(|line| line.contains("ollama model: missing (llama3.2)"))
        );
    }

    #[test]
    fn doctor_reports_cloud_auth_missing_before_probe() {
        let cfg = sample_config(Provider::Ollama, "https://ollama.com/v1", "", "gpt-oss:20b");

        let lines = runtime().block_on(doctor_ollama_lines(&cfg));

        assert_eq!(
            lines,
            vec!["ollama endpoint: auth missing for ollama cloud"]
        );
    }

    #[test]
    fn doctor_reports_incompatible_ollama_endpoint() {
        let cfg = sample_config(Provider::Ollama, "http://127.0.0.1:11434", "", "llama3.2");
        let lines = vec![format!(
            "ollama endpoint: {}",
            super::parse_ollama_probe_response(404, "missing", &cfg).expect_err("expected error")
        )];

        assert!(
            lines
                .iter()
                .any(|line| line.contains("incompatible endpoint"))
        );
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
            redaction_rules: crate::config::default_redaction_rules(),
            show_timing: true,
            use_env_proxy: false,
            timeout: Duration::from_secs(5),
            max_diff_bytes: 60_000,
            max_diff_tokens: Some(16_000),
            max_diff_tokens_explicit: false,
            model_context_tokens: None,
        }
    }
}
