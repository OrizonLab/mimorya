//! Questions on the company's documents (ADR-096), as the end-to-end model
//! plays them: recognised by their question, they are answered apart, so
//! that the reference scenarios queued for conversations keep their order
//! (`documents.spec.ts`). The model searches the documents, then answers
//! from the passage found, which the conversation cites.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{
    LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall, LlmUsage, StopReason,
};
use serde_json::json;

/// The question `documents.spec.ts` asks.
pub const ASKED: &str = "Sous quel délai une panne de chauffage est-elle prise en charge ?";
/// The answer it reads.
pub const ANSWER: &str = "D’après la charte qualité des interventions, une panne de chauffage \
signalée avant 12 h est prise en charge le jour même ; les autres demandes sont planifiées sous \
cinq jours ouvrés.";

/// The platform model: questions on the documents are played here, the
/// rest goes on.
pub struct WithDocuments {
    others: Arc<dyn LlmProvider>,
}

impl WithDocuments {
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
}

#[async_trait]
impl LlmProvider for WithDocuments {
    fn id(&self) -> &str {
        self.others.id()
    }

    fn model(&self) -> &str {
        self.others.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let asked = matches!(
            request.messages.first(),
            Some(LlmMessage::User { content }) if content.contains(ASKED)
        );
        if !asked {
            return self.others.complete(request).await;
        }
        // Searched already: the answer, from what was found.
        if matches!(
            request.messages.last(),
            Some(LlmMessage::ToolResults { .. })
        ) {
            return Ok(self.reply(ANSWER, Vec::new()));
        }
        let search = LlmToolCall {
            id: format!("documents-{}", uuid::Uuid::new_v4()),
            name: "document_search".into(),
            input: json!({ "query": "délai de prise en charge d’une panne de chauffage" }),
        };
        Ok(self.reply("", vec![search]))
    }
}
