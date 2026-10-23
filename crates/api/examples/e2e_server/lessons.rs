//! The rereading of finished runs (ADR-076), as the end-to-end model plays
//! it: recognised by its instructions, it is answered apart, always with the
//! same lesson, so that the reference scenarios queued for conversations
//! keep their order whichever runs a company has reread (`lessons.spec.ts`).

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{
    LlmError, LlmProvider, LlmRequest, LlmResponse, LlmToolCall, LlmUsage, StopReason,
};
use mimorya_agent::runtime::lesson::{LESSON_SYSTEM, LESSON_TOOL};
use serde_json::json;

/// The lesson every rereading proposes, which `lessons.spec.ts` validates.
const TITLE: &str = "Vérifier le planning avant de proposer un suivi";
const CONTENT: &str = "Consulter les comptes rendus de la semaine avant de proposer un créneau \
de suivi, puis demander la validation avant de l’appliquer.";

/// The platform model: conversations go to the scenarios, rereadings get
/// the lesson.
pub struct WithLessons {
    conversations: Arc<dyn LlmProvider>,
}

impl WithLessons {
    pub fn new(conversations: Arc<dyn LlmProvider>) -> Self {
        Self { conversations }
    }
}

#[async_trait]
impl LlmProvider for WithLessons {
    fn id(&self) -> &str {
        self.conversations.id()
    }

    fn model(&self) -> &str {
        self.conversations.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        if request.system != LESSON_SYSTEM {
            return self.conversations.complete(request).await;
        }
        Ok(LlmResponse {
            text: String::new(),
            tool_calls: vec![LlmToolCall {
                id: "lesson".into(),
                name: LESSON_TOOL.into(),
                input: json!({ "aucune": false, "titre": TITLE, "contenu": CONTENT }),
            }],
            stop_reason: StopReason::ToolUse,
            usage: LlmUsage {
                input_tokens: 100,
                output_tokens: 20,
                ..LlmUsage::default()
            },
            provider: self.conversations.id().into(),
            model: self.conversations.model().into(),
            native: None,
        })
    }
}
