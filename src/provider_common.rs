use crate::config::Config;
use reqwest::blocking::Client;

pub(crate) fn provider_debug_enabled(debug_provider: bool) -> bool {
    debug_provider
        || matches!(
            std::env::var("GIT_AI_COMMIT_DEBUG_PROVIDER")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("1" | "true" | "yes" | "on")
        )
}

pub(crate) fn truncate_debug_body(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(400).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

pub(crate) fn new_http_client(cfg: &Config) -> Result<Client, String> {
    let mut builder = Client::builder().timeout(cfg.timeout);
    if !cfg.use_env_proxy {
        builder = builder.no_proxy();
    }
    builder.build().map_err(|err| err.to_string())
}
