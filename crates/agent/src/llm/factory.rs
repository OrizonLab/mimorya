//! Builds a provider from configuration (server environment or tenant settings).

use std::sync::Arc;

use anyhow::bail;

use super::anthropic::{AnthropicOptions, AnthropicProvider};
use super::openai_compatible::{OpenAiCompatibleOptions, OpenAiCompatibleProvider};
use super::types::LlmProvider;

pub const PROVIDER_KINDS: [&str; 6] = [
    "anthropic",
    "openai",
    "openrouter",
    "gemini",
    "ollama",
    "openai_compatible",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    /// One of `PROVIDER_KINDS`.
    pub kind: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

fn default_endpoint(kind: &str) -> Option<&'static str> {
    match kind {
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "gemini" => Some("https://generativelanguage.googleapis.com/v1beta/openai/"),
        "ollama" => Some("http://localhost:11434/v1"),
        _ => None,
    }
}

pub fn create_provider(config: &ProviderConfig) -> anyhow::Result<Arc<dyn LlmProvider>> {
    if !PROVIDER_KINDS.contains(&config.kind.as_str()) {
        bail!("Unknown provider \"{}\"", config.kind);
    }
    if config.kind == "anthropic" {
        return Ok(Arc::new(AnthropicProvider::new(AnthropicOptions {
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: config.base_url.clone(),
            ..AnthropicOptions::default()
        })));
    }
    let Some(model) = config.model.clone() else {
        bail!(
            "Provider \"{}\" requires an explicit model name",
            config.kind
        );
    };
    let base_url = config
        .base_url
        .clone()
        .or_else(|| default_endpoint(&config.kind).map(Into::into));
    if config.kind == "openai_compatible" && base_url.is_none() {
        bail!("Provider \"openai_compatible\" requires a base URL");
    }
    Ok(Arc::new(OpenAiCompatibleProvider::new(
        OpenAiCompatibleOptions {
            id: config.kind.clone(),
            model,
            api_key: config.api_key.clone(),
            base_url,
        },
    )))
}
