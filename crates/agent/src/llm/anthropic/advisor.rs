//! Anthropic's advisor on the wire (ADR-207, step 3): its server tool, the
//! beta it is in (sent as long as the history holds its blocks, which the
//! API reads only with it), its consultations read from the iterations of a
//! response's usage, each billed at its model's prices, and the reasoning
//! dropped rather than refused once the tool is withdrawn, which edits the
//! tools that reasoning is bound to.

use serde_json::{Value, json};

use crate::llm::pricing::model_alias;
use crate::llm::types::{AdvisorTool, Consultation, LlmMessage, LlmUsage};

pub const ADVISOR_BETA: &str = "advisor-tool-2026-03-01";
pub const THINKING_BINDING_BETA: &str = "thinking-binding-controls-2026-08-01";

/// The models that reason unless told not to: their reasoning in the
/// history is bound to the tools it was produced with.
const THINKING_BY_DEFAULT: [&str; 11] = [
    "claude-fable-5",
    "claude-fable-5-1",
    "claude-fable-5-2",
    "claude-mythos-5",
    "claude-mythos-5-1",
    "claude-mythos-5-2",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
    "claude-sonnet-5",
    "claude-haiku-5-5",
];

/// The server tool, as the Messages API takes it.
pub fn advisor_tool(tool: &AdvisorTool) -> Value {
    let mut definition = json!({
        "type": "advisor_20260301",
        "name": "advisor",
        "model": tool.model,
        "max_uses": tool.max_uses,
    });
    if tool.cache {
        definition["caching"] = json!({ "type": "ephemeral", "ttl": "5m" });
    }
    definition
}

/// Whether the history holds the advisor's blocks: replayed as they came,
/// they are read only under its beta, even once the tool is withdrawn.
pub fn history_has_advisor(messages: &[LlmMessage]) -> bool {
    messages.iter().any(|message| match message {
        LlmMessage::Assistant {
            native: Some(native),
            ..
        } if native.provider == "anthropic" => native
            .content
            .as_array()
            .is_some_and(|blocks| blocks.iter().any(is_advisor_block)),
        _ => false,
    })
}

fn is_advisor_block(block: &Value) -> bool {
    block["type"] == "advisor_tool_result"
        || (block["type"] == "server_tool_use" && block["name"] == "advisor")
}

/// The thinking setting that drops the reasoning bound to tools since
/// changed, for a model that reasons by default (the same setting
/// otherwise); none for the others, whose history holds no reasoning.
pub fn dropping_unbound_thinking(model: &str) -> Option<Value> {
    THINKING_BY_DEFAULT.contains(&model_alias(model)).then(|| {
        json!({
            "type": "adaptive",
            "block_binding": { "prefix_mismatch_behavior": "drop_block" },
        })
    })
}

/// The advisor's consultations among the iterations of a response's usage;
/// the others are the model's own, already in the usage.
pub fn consultations(iterations: &[Value]) -> Vec<Consultation> {
    let tokens = |entry: &Value, key: &str| entry[key].as_u64().unwrap_or(0);
    iterations
        .iter()
        .filter(|entry| entry["type"] == "advisor_message")
        .map(|entry| Consultation {
            model: entry["model"].as_str().unwrap_or_default().to_string(),
            usage: LlmUsage {
                input_tokens: tokens(entry, "input_tokens"),
                output_tokens: tokens(entry, "output_tokens"),
                cache_read_tokens: tokens(entry, "cache_read_input_tokens"),
                cache_write_tokens: tokens(entry, "cache_creation_input_tokens"),
            },
        })
        .collect()
}

/// Whether a call of the advisor among `blocks` has no result yet: it runs
/// at the start of the next request, which must give the advisor again.
pub fn consultation_pending(blocks: &[Value]) -> bool {
    let answered: Vec<&Value> = blocks
        .iter()
        .filter(|block| block["type"] == "advisor_tool_result")
        .map(|block| &block["tool_use_id"])
        .collect();
    blocks
        .iter()
        .filter(|block| block["type"] == "server_tool_use" && block["name"] == "advisor")
        .any(|call| !answered.contains(&&call["id"]))
}
