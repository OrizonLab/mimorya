//! Request body for the Messages API, from the provider-neutral request.

use serde_json::{Map, Value, json};

use crate::llm::types::{LlmMessage, LlmRequest};

pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-opus-5-5";
pub const API_VERSION: &str = "2023-06-01";
pub const SERVER_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// Models accepting `fallbacks: "default"` on the Claude API.
const SERVER_FALLBACK_MODELS: [&str; 4] = [
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];

pub fn supports_server_fallback(model: &str) -> bool {
    SERVER_FALLBACK_MODELS.contains(&model)
}

/// Settings that shape every request of a provider instance.
#[derive(Debug, Clone)]
pub struct RequestSettings {
    pub model: String,
    pub max_output_tokens: u32,
    /// Opt into server-side fallback so that a policy decline is retried on
    /// the server-defined fallback model.
    pub server_side_fallback: bool,
    /// Stream tool inputs as generated; inputs are validated by the runtime anyway.
    pub eager_input_streaming: bool,
}

/// Assistant turns with native Anthropic content are replayed verbatim
/// (reasoning blocks included); others are rebuilt from text and tool calls.
pub fn to_anthropic_messages(messages: &[LlmMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| match message {
            LlmMessage::User { content } => json!({ "role": "user", "content": content }),
            LlmMessage::ToolResults { results } => {
                let blocks: Vec<Value> = results
                    .iter()
                    .map(|r| {
                        let mut block = json!({
                            "type": "tool_result",
                            "tool_use_id": r.tool_call_id,
                            "content": r.content,
                        });
                        if r.is_error {
                            block["is_error"] = json!(true);
                        }
                        block
                    })
                    .collect();
                json!({ "role": "user", "content": blocks })
            }
            LlmMessage::Assistant { native: Some(native), .. } if native.provider == "anthropic" => {
                json!({ "role": "assistant", "content": native.content })
            }
            LlmMessage::Assistant { content, tool_calls, .. } => {
                let mut blocks = Vec::new();
                if !content.is_empty() {
                    blocks.push(json!({ "type": "text", "text": content }));
                }
                for call in tool_calls {
                    let input = if call.input.is_null() { json!({}) } else { call.input.clone() };
                    blocks.push(json!({ "type": "tool_use", "id": call.id, "name": call.name, "input": input }));
                }
                if blocks.is_empty() {
                    json!({ "role": "assistant", "content": content })
                } else {
                    json!({ "role": "assistant", "content": blocks })
                }
            }
        })
        .collect()
}

pub fn build_body(settings: &RequestSettings, request: &LlmRequest) -> Value {
    let mut body = Map::new();
    body.insert("model".into(), json!(settings.model));
    body.insert(
        "max_tokens".into(),
        json!(
            request
                .max_output_tokens
                .unwrap_or(settings.max_output_tokens)
        ),
    );
    body.insert("system".into(), json!(request.system));
    body.insert(
        "messages".into(),
        Value::Array(to_anthropic_messages(&request.messages)),
    );
    // Caches the stable prefix (tools + system).
    body.insert("cache_control".into(), json!({ "type": "ephemeral" }));
    body.insert("stream".into(), json!(true));
    if !request.tools.is_empty() {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| {
                let mut tool = json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                });
                if settings.eager_input_streaming {
                    tool["eager_input_streaming"] = json!(true);
                }
                tool
            })
            .collect();
        body.insert("tools".into(), Value::Array(tools));
        body.insert("tool_choice".into(), json!({ "type": "auto" }));
    }
    if let Some(effort) = request.effort {
        body.insert("output_config".into(), json!({ "effort": effort }));
    }
    if settings.server_side_fallback {
        body.insert("fallbacks".into(), json!("default"));
    }
    Value::Object(body)
}
