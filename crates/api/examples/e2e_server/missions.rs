//! Missions (ADR-089), as the end-to-end model plays them: recognised by
//! their brief, they are answered apart, so that the reference scenarios
//! queued for conversations keep their order (`missions.spec.ts`). A
//! mission's first stage searches this week's interventions until its
//! budget is spent, then writes its progress note; the next stage gives the
//! final answer, which the review passes. A mission whose title asks to be
//! stopped waits a while at each call instead, for the browser to stop it.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Days, Utc};
use mimorya_agent::llm::{
    LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall, LlmUsage, StopReason,
};
use mimorya_connectors::demo_interventions::dates::{iso_date, monday_of};
use serde_json::json;

/// What only a mission's brief says.
const BRIEF: &str = "Tu mènes cette mission en arrière-plan";
/// What a stage's request for its note says.
const NOTE_ASKED: &str = "[Fin de l’étape";
/// What a later stage's request says.
const RESUMED: &str = "reprends à partir de ton dernier point";
/// The title of a mission the browser stops (`missions.spec.ts`).
const TO_STOP: &str = "(à arrêter)";
/// How long such a mission's model takes to answer.
const STOPPED_WAIT: Duration = Duration::from_secs(20);

/// The first stage's progress note, which the test reads.
pub const NOTE: &str = "Point d’étape : les interventions de la semaine sont relues. Reste à \
proposer leur suivi.";
/// The mission's final answer, which the test reads.
pub const ANSWER: &str = "Synthèse de la mission : les interventions de la semaine sont relues et \
leur suivi est proposé à l’équipe.";

/// The platform model: missions are played here, the rest goes on.
pub struct WithMissions {
    others: Arc<dyn LlmProvider>,
}

impl WithMissions {
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

fn says(message: &LlmMessage, words: &str) -> bool {
    matches!(message, LlmMessage::User { content } if content.contains(words))
}

fn search() -> LlmToolCall {
    let monday = monday_of(Utc::now().date_naive());
    let week = json!({ "from": iso_date(monday), "to": iso_date(monday + Days::new(4)) });
    LlmToolCall {
        id: format!("search-{}", uuid::Uuid::new_v4()),
        name: "interventions_search".into(),
        input: week,
    }
}

#[async_trait]
impl LlmProvider for WithMissions {
    fn id(&self) -> &str {
        self.others.id()
    }

    fn model(&self) -> &str {
        self.others.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        // A mission's final answer, reviewed against its criteria.
        if request.tools.iter().any(|t| t.name == "submit_verdict") {
            let verdict = LlmToolCall {
                id: "verdict".into(),
                name: "submit_verdict".into(),
                input: json!({ "pass": true, "issues": [] }),
            };
            return Ok(self.reply("", vec![verdict]));
        }
        let Some(first) = request.messages.first() else {
            return self.others.complete(request).await;
        };
        if !says(first, BRIEF) {
            return self.others.complete(request).await;
        }
        if says(first, TO_STOP) {
            tokio::time::sleep(STOPPED_WAIT).await;
            return Ok(self.reply("", vec![search()]));
        }
        let last = request.messages.last().unwrap_or(first);
        if says(last, NOTE_ASKED) {
            return Ok(self.reply(NOTE, Vec::new()));
        }
        if request.messages.iter().any(|m| says(m, RESUMED)) {
            return Ok(self.reply(ANSWER, Vec::new()));
        }
        Ok(self.reply("", vec![search()]))
    }
}
