//! A skill the agent proposes (ADR-100), as the end-to-end model plays it:
//! recognised by its question, it is answered apart, so that the reference
//! scenarios queued for conversations keep their order
//! (`skill-proposals.spec.ts`). The model proposes a new skill; once the
//! person validated its text, it says the proposal went to the skills'
//! administrators.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall};
use serde_json::json;

use crate::played;

/// The question `skill-proposals.spec.ts` asks.
pub const ASKED: &str =
    "Propose aux administrateurs notre façon de relancer un fournisseur dont la pièce tarde.";
/// The answer once the proposal is validated.
pub const ANSWER: &str =
    "La procédure de relance d’un fournisseur est proposée aux administrateurs des compétences.";

/// The platform model: the proposed skill is played here, the rest goes on.
pub struct WithSkillProposals {
    others: Arc<dyn LlmProvider>,
}

impl WithSkillProposals {
    pub fn new(others: Arc<dyn LlmProvider>) -> Self {
        Self { others }
    }
}

#[async_trait]
impl LlmProvider for WithSkillProposals {
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
        // Validated and run: the answer.
        if matches!(
            request.messages.last(),
            Some(LlmMessage::ToolResults { .. })
        ) {
            return Ok(played::reply(self, ANSWER, Vec::new()));
        }
        let proposal = LlmToolCall {
            id: format!("skill-proposals-{}", uuid::Uuid::new_v4()),
            name: "skill_propose".into(),
            input: json!({
                "name": "relance-fournisseur",
                "description": "Relancer un fournisseur dont la pièce commandée tarde.",
                "content": "# Relance d’un fournisseur\n1. Retrouver la commande et sa date promise.\n2. Appeler le fournisseur, puis confirmer par écrit.",
                "rationale": "Procédure suivie avec succès dans cette conversation.",
            }),
        };
        Ok(played::reply(self, "", vec![proposal]))
    }
}
