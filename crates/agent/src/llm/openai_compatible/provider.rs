//! Adapter for OpenAI-compatible Chat Completions endpoints: OpenAI,
//! OpenRouter, Google Gemini (OpenAI-compatible endpoint), Mistral, and local
//! runtimes (Ollama, vLLM, LM Studio). Non-streaming for maximal
//! compatibility across servers; the final text is reported as a single delta.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::errors::{from_status, malformed, network};
use super::messages::{map_finish_reason, parse_arguments, to_openai_messages};
use crate::llm::types::{LlmError, LlmProvider, LlmRequest, LlmResponse, LlmToolCall, LlmUsage};

pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// No `Debug`: it would print the API key.
#[derive(Clone)]
pub struct OpenAiCompatibleOptions {
    /// Identifier used in usage records, e.g. "openai", "openrouter", "ollama".
    pub id: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

pub struct OpenAiCompatibleProvider {
    http: reqwest::Client,
    id: String,
    model: String,
    api_key: String,
    base_url: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(options: OpenAiCompatibleOptions) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(600))
                .build()
                .expect("HTTP client"),
            id: options.id,
            model: options.model,
            // Local runtimes accept any key.
            api_key: options.api_key.unwrap_or_else(|| "not-needed".into()),
            base_url: options
                .base_url
                .unwrap_or_else(|| OPENAI_BASE_URL.into())
                .trim_end_matches('/')
                .to_string(),
        }
    }

    pub fn build_body(&self, request: &LlmRequest) -> Value {
        let mut body = json!({
            "model": self.model,
            "messages": to_openai_messages(&request.system, &request.messages),
        });
        if !request.tools.is_empty() {
            body["tools"] = request
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": { "name": t.name, "description": t.description, "parameters": t.input_schema },
                    })
                })
                .collect();
            body["tool_choice"] = json!("auto");
        }
        body
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&self.build_body(request))
            .send()
            .await
            .map_err(|_| network(&self.id))?;
        let status = response.status();
        let body = response.text().await.map_err(|_| network(&self.id))?;
        if !status.is_success() {
            return Err(from_status(&self.id, status.as_u16(), &body));
        }
        let completion: Value = serde_json::from_str(&body).map_err(|_| malformed(&self.id))?;
        let choice = completion["choices"]
            .get(0)
            .ok_or_else(|| malformed(&self.id))?;
        let message = &choice["message"];
        let text = message["content"].as_str().unwrap_or_default().to_string();
        if !text.is_empty() {
            request.emit_text(&text);
        }
        let tool_calls: Vec<LlmToolCall> = message["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["type"] == "function")
            .map(|c| LlmToolCall {
                id: c["id"].as_str().unwrap_or_default().into(),
                name: c["function"]["name"].as_str().unwrap_or_default().into(),
                input: parse_arguments(c["function"]["arguments"].as_str().unwrap_or_default()),
            })
            .collect();
        let usage = &completion["usage"];
        // `prompt_tokens` includes the cached ones.
        let cached = usage["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0);
        Ok(LlmResponse {
            stop_reason: map_finish_reason(
                choice["finish_reason"].as_str(),
                !tool_calls.is_empty(),
            ),
            text,
            tool_calls,
            usage: LlmUsage {
                input_tokens: usage["prompt_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_sub(cached),
                output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
                cache_read_tokens: cached,
                cache_write_tokens: 0,
            },
            provider: self.id.clone(),
            model: completion["model"]
                .as_str()
                .filter(|m| !m.is_empty())
                .unwrap_or(&self.model)
                .to_string(),
            native: None,
        })
    }
}
