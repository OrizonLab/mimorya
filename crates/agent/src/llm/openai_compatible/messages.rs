//! Conversion between the provider-neutral message format and Chat
//! Completions messages.

use serde_json::{Value, json};

use crate::llm::types::{LlmMessage, StopReason};

/// Invalid JSON is passed on as-is: schema validation rejects it and tells the model.
pub fn parse_arguments(raw: &str) -> Value {
    if raw.is_empty() {
        return json!({});
    }
    serde_json::from_str(raw).unwrap_or_else(|_| json!({ "__invalid_json__": raw }))
}

pub fn to_openai_messages(system: &str, messages: &[LlmMessage]) -> Vec<Value> {
    let mut result = vec![json!({ "role": "system", "content": system })];
    for message in messages {
        match message {
            LlmMessage::User { content } => {
                result.push(json!({ "role": "user", "content": content }))
            }
            LlmMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                let mut entry = json!({
                    "role": "assistant",
                    "content": if content.is_empty() { Value::Null } else { json!(content) },
                });
                if !tool_calls.is_empty() {
                    entry["tool_calls"] = tool_calls
                        .iter()
                        .map(|c| {
                            let input = if c.input.is_null() {
                                json!({})
                            } else {
                                c.input.clone()
                            };
                            json!({
                                "id": c.id,
                                "type": "function",
                                "function": { "name": c.name, "arguments": input.to_string() },
                            })
                        })
                        .collect();
                }
                result.push(entry);
            }
            LlmMessage::ToolResults { results } => {
                for r in results {
                    result.push(json!({ "role": "tool", "tool_call_id": r.tool_call_id, "content": r.content }));
                }
            }
        }
    }
    result
}

pub fn map_finish_reason(reason: Option<&str>, has_tool_calls: bool) -> StopReason {
    if has_tool_calls || reason == Some("tool_calls") {
        return StopReason::ToolUse;
    }
    match reason {
        Some("stop") => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::Refusal,
        _ => StopReason::Other,
    }
}
