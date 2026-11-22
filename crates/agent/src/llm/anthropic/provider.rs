//! Anthropic adapter (Messages API over HTTP).
//!
//! - Streams every request (long agentic turns) and reports text deltas.
//! - Replays assistant turns verbatim from their native content blocks, as
//!   required for reasoning blocks bound to the conversation.
//! - Enables prompt caching on the stable prefix (tools + system).
//! - Opts into server-side fallback on supported models.
//! - Gives the model Anthropic's advisor when the run does (ADR-207).

use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;

use super::errors::{from_status, network};
use super::request::{
    API_VERSION, DEFAULT_ANTHROPIC_MODEL, RequestSettings, betas, build_body,
    supports_server_fallback,
};
use super::sse::SseParser;
use super::stream::MessageAccumulator;
use crate::llm::types::{LlmError, LlmProvider, LlmRequest, LlmResponse};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// No `Debug`: it would print the API key.
#[derive(Clone, Default)]
pub struct AnthropicOptions {
    /// Defaults to `ANTHROPIC_API_KEY`.
    pub api_key: Option<String>,
    pub model: Option<String>,
    /// Defaults to `ANTHROPIC_BASE_URL`, then the public API.
    pub base_url: Option<String>,
    /// Default 64k: requests are streamed, so large caps do not hit HTTP timeouts.
    pub max_output_tokens: Option<u32>,
    pub server_side_fallback: Option<bool>,
    pub eager_input_streaming: Option<bool>,
}

pub struct AnthropicProvider {
    http: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
    settings: RequestSettings,
}

impl AnthropicProvider {
    pub fn new(options: AnthropicOptions) -> Self {
        let model = options
            .model
            .unwrap_or_else(|| DEFAULT_ANTHROPIC_MODEL.into());
        let server_side_fallback =
            options.server_side_fallback.unwrap_or(true) && supports_server_fallback(&model);
        let base_url = given(options.base_url)
            .or_else(|| given(std::env::var("ANTHROPIC_BASE_URL").ok()))
            .unwrap_or_else(|| DEFAULT_BASE_URL.into());
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(300))
                .build()
                .expect("HTTP client"),
            api_key: given(options.api_key)
                .or_else(|| given(std::env::var("ANTHROPIC_API_KEY").ok())),
            base_url: base_url.trim_end_matches('/').to_string(),
            settings: RequestSettings {
                model,
                max_output_tokens: options.max_output_tokens.unwrap_or(64_000),
                server_side_fallback,
                eager_input_streaming: options.eager_input_streaming.unwrap_or(true),
            },
        }
    }
}

/// A value as given, an empty one counting as none: a stack passes
/// `ANTHROPIC_BASE_URL=` when its operator sets nothing (ADR-221), which
/// must mean Anthropic's own address, not an empty one.
fn given(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.settings.model
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let mut http = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("anthropic-version", API_VERSION)
            .header("accept", "text/event-stream")
            .json(&build_body(&self.settings, request));
        if let Some(key) = &self.api_key {
            http = http.header("x-api-key", key);
        }
        let betas = betas(&self.settings, request);
        if !betas.is_empty() {
            http = http.header("anthropic-beta", betas.join(","));
        }

        let response = http.send().await.map_err(|_| network())?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(from_status(status.as_u16(), &body));
        }

        let mut parser = SseParser::default();
        let mut message = MessageAccumulator::new(&self.settings.model);
        let on_text = |text: &str| request.emit_text(text);
        let mut bytes = response.bytes_stream();
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.map_err(|_| network())?;
            for event in parser.push(&chunk) {
                message.apply(&event, &on_text)?;
            }
        }
        message.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_address_or_key_counts_as_none() {
        assert_eq!(given(Some(String::new())), None);
        assert_eq!(given(Some("  ".into())), None);
        assert_eq!(given(None), None);
        assert_eq!(
            given(Some("https://gw.example".into())).as_deref(),
            Some("https://gw.example")
        );
        // An empty address given falls back to Anthropic's own.
        let provider = AnthropicProvider::new(AnthropicOptions {
            base_url: Some(String::new()),
            api_key: Some("cle".into()),
            ..AnthropicOptions::default()
        });
        let expected = given(std::env::var("ANTHROPIC_BASE_URL").ok())
            .unwrap_or_else(|| DEFAULT_BASE_URL.into());
        assert_eq!(provider.base_url, expected.trim_end_matches('/'));
    }
}
