//! What a person writes to the company's Slack application (ADR-127,
//! ADR-128), as the end-to-end model plays it: recognised by its first
//! question, the conversation is answered apart, so that the reference
//! scenarios queued for conversations keep their order
//! (`slack-conversations.spec.ts`). Its follow-up, written in the thread,
//! is answered from what came before.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse};

use crate::played;

/// The question `slack-conversations.spec.ts` writes in Slack.
const ASKED: &str = "Quelles interventions sont prévues demain à Lyon ?";
/// Its answer, in Markdown: written in Slack's markup there. It cites no
/// identifier, which the answer's review would find in no tool's result.
const ANSWER: &str = "**Deux interventions** sont prévues demain à Lyon : une mise en service \
le matin et une visite d’entretien l’après-midi.";
/// Its follow-up, written in the thread.
const FOLLOW_UP: &str = "Laquelle commence en premier ?";
const FOLLOW_UP_ANSWER: &str = "La mise en service commence la première, à 8 h.";

/// The platform model: conversations written in Slack are played here, the
/// rest goes on.
pub struct WithSlack {
    others: Arc<dyn LlmProvider>,
}

impl WithSlack {
    pub fn new(others: Arc<dyn LlmProvider>) -> Self {
        Self { others }
    }
}

#[async_trait]
impl LlmProvider for WithSlack {
    fn id(&self) -> &str {
        self.others.id()
    }

    fn model(&self) -> &str {
        self.others.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let asked = request
            .messages
            .first()
            .is_some_and(|m| played::says(m, ASKED));
        if !asked {
            return self.others.complete(request).await;
        }
        let last_turn = request
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m, LlmMessage::User { .. }));
        let answer = if last_turn.is_some_and(|m| played::says(m, FOLLOW_UP)) {
            FOLLOW_UP_ANSWER
        } else {
            ANSWER
        };
        Ok(played::reply(self, answer, Vec::new()))
    }
}
