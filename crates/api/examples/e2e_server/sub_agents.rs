//! A question handed to a sub-agent (ADR-098), as the end-to-end model
//! plays it: recognised by its question, it is answered apart, so that the
//! reference scenarios queued for conversations keep their order
//! (`sub-agents.spec.ts`). The model hands a research to a sub-agent, which
//! searches the documents and returns its summary, then answers from it.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall};
use serde_json::json;

use crate::played;

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
    request
        .messages
        .first()
        .is_some_and(|m| played::says(m, text))
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
            (true, true) => played::reply(self, SUMMARY, Vec::new()),
            (true, false) => {
                let search =
                    json!({ "query": "délai de prise en charge d’une panne de chauffage" });
                played::reply(self, "", vec![Self::call("document_search", search)])
            }
            (false, true) => played::reply(self, ANSWER, Vec::new()),
            (false, false) => {
                let task = json!({ "profile": "research", "task": TASK });
                played::reply(self, "", vec![Self::call("delegate_task", task)])
            }
        })
    }
}
