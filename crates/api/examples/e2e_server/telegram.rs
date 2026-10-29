//! What a person writes to the company's Telegram bot (ADR-135), as the
//! end-to-end model plays it: recognised by their first question, the
//! conversation is answered apart, so that the reference scenarios queued
//! for conversations keep their order (`telegram-conversations.spec.ts`).
//! Its follow-up is answered from what came before; the question that opens
//! the next conversation, after `/nouvelle`, apart too.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse};

use crate::played;

/// The question `telegram-conversations.spec.ts` writes to the bot.
const ASKED: &str = "Quelles visites sont prévues vendredi à Villeurbanne ?";
/// Its answer, in Markdown: written in Telegram's HTML there. It cites no
/// identifier, which the answer's review would find in no tool's result.
const ANSWER: &str = "**Une visite** est prévue vendredi à Villeurbanne : l’entretien d’une \
pompe de relevage.";
/// Its follow-up, in the same conversation.
const FOLLOW_UP: &str = "À quelle heure ?";
const FOLLOW_UP_ANSWER: &str = "La visite commence à 10 h.";
/// The question that opens the next conversation, and its answer.
const NEXT: &str = "Et la semaine prochaine à Villeurbanne ?";
const NEXT_ANSWER: &str = "Rien n’est encore prévu la semaine prochaine à Villeurbanne.";

/// The platform model: conversations written to the bot are played here,
/// the rest goes on.
pub struct WithTelegram {
    others: Arc<dyn LlmProvider>,
}

impl WithTelegram {
    pub fn new(others: Arc<dyn LlmProvider>) -> Self {
        Self { others }
    }
}

#[async_trait]
impl LlmProvider for WithTelegram {
    fn id(&self) -> &str {
        self.others.id()
    }

    fn model(&self) -> &str {
        self.others.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let first = request.messages.first();
        let answer = if first.is_some_and(|m| played::says(m, NEXT)) {
            NEXT_ANSWER
        } else if first.is_some_and(|m| played::says(m, ASKED)) {
            let last_turn =
                (request.messages.iter().rev()).find(|m| matches!(m, LlmMessage::User { .. }));
            if last_turn.is_some_and(|m| played::says(m, FOLLOW_UP)) {
                FOLLOW_UP_ANSWER
            } else {
                ANSWER
            }
        } else {
            return self.others.complete(request).await;
        };
        Ok(played::reply(self, answer, Vec::new()))
    }
}
