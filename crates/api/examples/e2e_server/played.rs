//! What the models that play a scenario apart share (`documents`, `hooks`,
//! `missions`, `skill_evaluations`, `skill_proposals`, `slack`,
//! `sub_agents`): a reply in the name of the model they stand in front of,
//! and a turn recognised by what it says.

use mimorya_agent::llm::{LlmMessage, LlmProvider, LlmResponse, LlmToolCall, LlmUsage, StopReason};

/// `text` and `tool_calls` answered as `model`, at a fixed usage.
pub fn reply(model: &dyn LlmProvider, text: &str, tool_calls: Vec<LlmToolCall>) -> LlmResponse {
    let stop_reason = if tool_calls.is_empty() {
        StopReason::EndTurn
    } else {
        StopReason::ToolUse
    };
    LlmResponse {
        text: text.into(),
        tool_calls,
        stop_reason,
        usage: LlmUsage {
            input_tokens: 100,
            output_tokens: 20,
            ..LlmUsage::default()
        },
        provider: model.id().into(),
        model: model.model().into(),
        native: None,
    }
}

/// Whether `message` is a user turn that says `words`.
pub fn says(message: &LlmMessage, words: &str) -> bool {
    matches!(message, LlmMessage::User { content } if content.contains(words))
}
