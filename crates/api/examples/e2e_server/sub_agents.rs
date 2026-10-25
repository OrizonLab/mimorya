//! A question handed to a sub-agent (ADR-098), as the end-to-end model
//! plays it: recognised by its question, it is answered apart, so that the
//! reference scenarios queued for conversations keep their order
//! (`sub-agents.spec.ts`). The model hands a research to a sub-agent, which
//! searches the documents and returns its summary, then answers from it.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{
    LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall, LlmUsage, StopReason,
};
use serde_json::json;

/// The question `sub-agents.spec.ts` asks.
pub const ASKED: &str =
    "Fais rechercher par un sous-agent le délai de prise en charge d’une panne de chauffage.";
/// The task the model hands to the sub-agent.
pub const TASK: &str =
    "Trouver dans les documents le délai de prise en charge d’une panne de chauffage";
/// The sub-agent's summary.
const SUMMARY: &str = "Charte qualité des interventions : une panne de chauffage signalée avant \
12 h est prise en charge le jour même.";
/// The answer the spec reads.
pub const ANSWER: &str = "D’après la recherche confiée à un sous-agent, une panne de chauffage \
signalée avant 12 h est prise en charge le jour même.";

/// What marks a sub-agent's instructions (`delegation/prompt.rs`).
const SUB_AGENT_ROLE: &str = "## Rôle : sous-agent";

/// The platform model: the delegated question and its sub-agent are played
/// here, the rest goes on.
pub struct WithSubAgents {
    others: Arc<dyn LlmProvider>,
}

impl WithSubAgents {
    pub fn new(others: Arc<dyn LlmProvider>) -> Self {
        Self { others }
    }

    fn reply(&self, text: &str, tool_calls: Vec<LlmToolCall>) -> LlmResponse {
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
            provider: self.others.id().into(),
            model: self.others.model().into(),
            native: None,
        }
    }

    fn call(name: &str, input: serde_json::Value) -> LlmToolCall {
        LlmToolCall {
            id: format!("sub-agents-{}", uuid::Uuid::new_v4()),
            name: name.into(),
            input,
        }
    }
}

/// Whether the request's first turn says `text`.
fn asks(request: &LlmRequest, text: &str) -> bool {
    matches!(
        request.messages.first(),
        Some(LlmMessage::User { content }) if content.contains(text)
    )
}

#[async_trait]
impl LlmProvider for WithSubAgents {
    fn id(&self) -> &str {
        self.others.id()
    }

    fn model(&self) -> &str {
        self.others.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let sub_agent = request.system.contains(SUB_AGENT_ROLE) && asks(request, TASK);
        if !sub_agent && !asks(request, ASKED) {
            return self.others.complete(request).await;
        }
        let answered = matches!(
            request.messages.last(),
            Some(LlmMessage::ToolResults { .. })
        );
        Ok(match (sub_agent, answered) {
            (true, true) => self.reply(SUMMARY, Vec::new()),
            (true, false) => {
                let search =
                    json!({ "query": "délai de prise en charge d’une panne de chauffage" });
                self.reply("", vec![Self::call("document_search", search)])
            }
            (false, true) => self.reply(ANSWER, Vec::new()),
            (false, false) => {
                let task = json!({ "profile": "research", "task": TASK });
                self.reply("", vec![Self::call("delegate_task", task)])
            }
        })
    }
}
