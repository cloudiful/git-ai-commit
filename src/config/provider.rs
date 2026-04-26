use reqwest::Url;
use std::net::IpAddr;

pub const DEFAULT_OLLAMA_API_BASE: &str = "http://localhost:11434";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Provider {
    #[default]
    OpenAiCompatible,
    Ollama,
    AnthropicCompatible,
}

impl Provider {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai" | "openai-compatible" => Some(Self::OpenAiCompatible),
            "ollama" => Some(Self::Ollama),
            "anthropic" | "anthropic-compatible" => Some(Self::AnthropicCompatible),
            _ => None,
        }
    }

    pub fn as_config_value(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai-compatible",
            Self::Ollama => "ollama",
            Self::AnthropicCompatible => "anthropic-compatible",
        }
    }
}

pub fn is_loopback_url(base: &str) -> bool {
    let Ok(url) = Url::parse(base.trim()) else {
        return false;
    };

    let Some(host) = url.host_str() else {
        return false;
    };

    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|addr| addr.is_loopback())
            .unwrap_or(false)
}

pub fn is_ollama_cloud_url(base: &str) -> bool {
    let Ok(url) = Url::parse(base.trim()) else {
        return false;
    };

    matches!(url.host_str(), Some(host) if host.eq_ignore_ascii_case("ollama.com"))
}

pub fn is_openrouter_url(base: &str) -> bool {
    let Ok(url) = Url::parse(base.trim()) else {
        return false;
    };

    matches!(
        url.host_str(),
        Some(host)
            if host.eq_ignore_ascii_case("openrouter.ai")
                || host.eq_ignore_ascii_case("api.openrouter.ai")
    )
}

pub fn is_anthropic_compatible_url(base: &str) -> bool {
    let Ok(url) = Url::parse(base.trim()) else {
        return false;
    };

    let path = url.path().trim_end_matches('/').to_ascii_lowercase();
    path == "/anthropic" || path.ends_with("/anthropic")
}
