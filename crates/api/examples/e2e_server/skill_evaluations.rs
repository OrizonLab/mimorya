//! A skill's version tried on its case (ADR-104, ADR-105), as the
//! end-to-end model plays it: recognised by the case's request, it is
//! answered apart, so that the reference scenarios queued for
//! conversations keep their order (`skill-evaluations.spec.ts`). The agent
//! loads the skill, then answers as the version played says: the version
//! that says to cite no delay makes it answer without one, which the judge
//! fails, while the active version passes. A regression. The model has no
//! price: what the company's evaluations spent is written at start, for the
//! screens to show it (`evaluation-limit.spec.ts`, ADR-155).

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::llm::{LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolCall};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::evaluation_spending::add_evaluation_spending;
use serde_json::json;

use crate::played;

/// The request of the case `skill-evaluations.spec.ts` writes.
pub const ASKED: &str = "Quels retards faut-il signaler cette semaine ? (évaluation e2e)";
/// The skill it writes, which the agent loads.
const SKILL: &str = "retards-e2e";
/// What the version that regresses says.
const REGRESSING: &str = "Ne citer aucun retard.";
/// The answer the active version leads to, and the other one.
const CITED: &str = "Deux retards : l’intervention de Lyon et celle de Nantes.";
const NONE_CITED: &str = "Aucun retard à signaler.";
/// The issue the judge finds in the answer that cites none.
const ISSUE: &str = "Les retards de la semaine ne sont pas cités.";
/// What the demo company's evaluations spent today, as the screens show it
/// (1,234 USD), well under the platform's limit.
const SPENT_MICROS: i64 = 1_234_000;

/// Writes what the demo company's evaluations spent today.
pub async fn leave(app: &mimorya_api::app::App) -> anyhow::Result<()> {
    let mut tx = app.services.db.begin(tenant::ID).await?;
    add_evaluation_spending(&mut tx, tenant::ID, SPENT_MICROS, 40).await?;
    tx.commit().await?;
    Ok(())
}

/// The platform model: the case is played here, the rest goes on.
pub struct WithSkillEvaluations {
    others: Arc<dyn LlmProvider>,
}

impl WithSkillEvaluations {
    pub fn new(others: Arc<dyn LlmProvider>) -> Self {
        Self { others }
    }
}

#[async_trait]
impl LlmProvider for WithSkillEvaluations {
    fn id(&self) -> &str {
        self.others.id()
    }

    fn model(&self) -> &str {
        self.others.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let Some(first) = request.messages.first() else {
            return self.others.complete(request).await;
        };
        if !played::says(first, ASKED) {
            return self.others.complete(request).await;
        }
        // The judge, given the answer to check.
        if request.tools.iter().any(|t| t.name == "submit_verdict") {
            let passes = !played::says(first, NONE_CITED);
            let issues: Vec<&str> = if passes { Vec::new() } else { vec![ISSUE] };
            let verdict = call(
                "submit_verdict",
                json!({ "pass": passes, "issues": issues }),
            );
            return Ok(played::reply(self, "", vec![verdict]));
        }
        // The skill loaded: the answer it leads to.
        if let Some(LlmMessage::ToolResults { results }) = request.messages.last() {
            let regresses = results.iter().any(|r| r.content.contains(REGRESSING));
            let answer = if regresses { NONE_CITED } else { CITED };
            return Ok(played::reply(self, answer, Vec::new()));
        }
        let load = call("skill_read", json!({ "name": SKILL }));
        Ok(played::reply(self, "", vec![load]))
    }
}

fn call(name: &str, input: serde_json::Value) -> LlmToolCall {
    LlmToolCall {
        id: format!("skill-evaluations-{}", uuid::Uuid::new_v4()),
        name: name.into(),
        input,
    }
}
