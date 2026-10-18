//! Rebuilds a complete message from the events of a Messages API stream, the
//! way the official SDKs do: content blocks are kept exactly as returned so
//! that they can be replayed verbatim on the next turn.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::errors::{from_stream_error, malformed, network};
use super::sse::SseEvent;
use crate::llm::types::{LlmError, LlmResponse, LlmToolCall, LlmUsage, NativeContent, StopReason};

#[derive(Debug, Default)]
pub struct MessageAccumulator {
    model: String,
    usage: LlmUsage,
    stop_reason: Option<String>,
    blocks: Vec<Value>,
    /// Tool inputs being streamed, by block index.
    partial_json: HashMap<usize, String>,
    complete: bool,
}

fn set_usage(target: &mut u64, usage: &Value, key: &str) {
    if let Some(value) = usage[key].as_u64() {
        *target = value;
    }
}

impl MessageAccumulator {
    pub fn new(requested_model: &str) -> Self {
        Self {
            model: requested_model.into(),
            ..Self::default()
        }
    }

    /// Applies one stream event; text deltas are passed to `on_text`.
    pub fn apply(&mut self, event: &SseEvent, on_text: &dyn Fn(&str)) -> Result<(), LlmError> {
        if event.event == "ping" {
            return Ok(());
        }
        let data: Value = serde_json::from_str(&event.data).map_err(|_| malformed())?;
        let kind = data["type"].as_str().unwrap_or(&event.event).to_string();
        let index = data["index"].as_u64().map(|i| i as usize);
        match kind.as_str() {
            "message_start" => {
                let message = &data["message"];
                if let Some(model) = message["model"].as_str() {
                    self.model = model.into();
                }
                self.update_usage(&message["usage"]);
            }
            "content_block_start" => {
                let index = index.ok_or_else(malformed)?;
                if self.blocks.len() <= index {
                    self.blocks.resize(index + 1, Value::Null);
                }
                let block = data["content_block"].clone();
                if matches!(block["type"].as_str(), Some("tool_use" | "server_tool_use")) {
                    self.partial_json.insert(index, String::new());
                }
                self.blocks[index] = block;
            }
            "content_block_delta" => {
                let index = index.ok_or_else(malformed)?;
                let block = self.blocks.get_mut(index).ok_or_else(malformed)?;
                let delta = &data["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => {
                        let text = delta["text"].as_str().unwrap_or_default();
                        append(block, "text", text);
                        on_text(text);
                    }
                    Some("input_json_delta") => {
                        let part = delta["partial_json"].as_str().unwrap_or_default();
                        self.partial_json.entry(index).or_default().push_str(part);
                    }
                    Some("thinking_delta") => {
                        append(
                            block,
                            "thinking",
                            delta["thinking"].as_str().unwrap_or_default(),
                        );
                    }
                    Some("signature_delta") => {
                        block["signature"] = delta["signature"].clone();
                    }
                    Some("citations_delta") => {
                        if !block["citations"].is_array() {
                            block["citations"] = json!([]);
                        }
                        if let Some(list) = block["citations"].as_array_mut() {
                            list.push(delta["citation"].clone());
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = index.ok_or_else(malformed)?;
                if let Some(raw) = self.partial_json.remove(&index) {
                    let input = if raw.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&raw).map_err(|_| malformed())?
                    };
                    if let Some(block) = self.blocks.get_mut(index) {
                        block["input"] = input;
                    }
                }
            }
            "message_delta" => {
                if let Some(reason) = data["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.into());
                }
                self.update_usage(&data["usage"]);
            }
            "message_stop" => self.complete = true,
            "error" => return Err(from_stream_error(&data)),
            _ => {}
        }
        Ok(())
    }

    fn update_usage(&mut self, usage: &Value) {
        set_usage(&mut self.usage.input_tokens, usage, "input_tokens");
        set_usage(&mut self.usage.output_tokens, usage, "output_tokens");
        set_usage(
            &mut self.usage.cache_read_tokens,
            usage,
            "cache_read_input_tokens",
        );
        set_usage(
            &mut self.usage.cache_write_tokens,
            usage,
            "cache_creation_input_tokens",
        );
    }

    pub fn finish(self) -> Result<LlmResponse, LlmError> {
        // A stream cut before `message_stop` is a transport failure (retryable).
        if !self.complete {
            return Err(network());
        }
        let blocks: Vec<Value> = self.blocks.into_iter().filter(|b| !b.is_null()).collect();
        let text = blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<String>();
        let tool_calls = blocks
            .iter()
            .filter(|b| b["type"] == "tool_use")
            .map(|b| LlmToolCall {
                id: b["id"].as_str().unwrap_or_default().into(),
                name: b["name"].as_str().unwrap_or_default().into(),
                input: b["input"].clone(),
            })
            .collect();
        Ok(LlmResponse {
            text,
            tool_calls,
            stop_reason: map_stop_reason(self.stop_reason.as_deref()),
            usage: self.usage,
            provider: "anthropic".into(),
            model: self.model,
            native: Some(NativeContent {
                provider: "anthropic".into(),
                content: Value::Array(blocks),
            }),
        })
    }
}

fn append(block: &mut Value, key: &str, text: &str) {
    let current = block[key].as_str().unwrap_or_default();
    block[key] = Value::String(format!("{current}{text}"));
}

pub fn map_stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("end_turn" | "stop_sequence") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens" | "model_context_window_exceeded") => StopReason::MaxTokens,
        Some("refusal") => StopReason::Refusal,
        _ => StopReason::Other,
    }
}
