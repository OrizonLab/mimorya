//! The company's detection of instructions meant for an AI (ADR-213, D6),
//! asked by the engine of each result of a tool that brings external
//! content, before the model reads it. Its settings are read at each
//! result. It sends the tool and the start of the content, nothing else.
//! Active, a content said to hold such instructions is marked and its run
//! held suspect; one no engine reads within the wait holds the run suspect
//! too; observed, it is asked, counted and journaled, without effect; off,
//! or its settings unread, nothing is asked. Its cost is counted in the
//! run. It is no absolute protection.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::decision::{
    DecisionMode, DecisionOutcome, DecisionUsage, holds_instructions, injection_request,
};
use mimorya_agent::runtime::{ContentScreen, ScreenReview, ScreenVerdict};

use super::db_ports::{DbPorts, RunScope};
use super::usage_decisions::{MAX_WAIT, usage_decisions};
use crate::services::llm::{LlmService, Models};

pub(super) struct DbContentScreen {
    llm: Arc<LlmService>,
    models: Arc<Models>,
    scope: RunScope,
}

impl DbContentScreen {
    /// The detection of the run `ports` serve.
    pub(super) fn of(ports: &DbPorts) -> Self {
        Self {
            llm: ports.llm.clone(),
            models: ports.models.clone(),
            scope: ports.scope,
        }
    }
}

#[async_trait]
impl ContentScreen for DbContentScreen {
    async fn screen(&self, tool: &str, content: &str) -> ScreenReview {
        let usage = DecisionUsage::InjectionDetection;
        let Some((decisions, policy)) =
            usage_decisions(&self.llm, &self.models, self.scope, usage).await
        else {
            return ScreenReview::default();
        };
        let request = injection_request(tool, content);
        // Each engine within what is left of the wait: a call that answered
        // is counted, whatever follows.
        let (outcome, counted) = decisions
            .service
            .decide_within(&request, Some(MAX_WAIT))
            .await;
        let verdict = verdict_of(&outcome, policy.threshold);
        tracing::info!(
            target: "mimorya::decision",
            usage = usage.as_str(),
            mode = policy.mode.as_str(),
            tool,
            verdict = ?verdict,
            "External content screened"
        );
        ScreenReview {
            verdict: in_effect(policy.mode, verdict),
            counted,
        }
    }
}

/// What the decision says: instructions at `threshold`, or, no engine
/// having decided, the usage's prudent value.
fn verdict_of(outcome: &DecisionOutcome, threshold: f64) -> ScreenVerdict {
    match outcome {
        DecisionOutcome::Decided(decided) if holds_instructions(decided, threshold) => {
            ScreenVerdict::Suspect
        }
        DecisionOutcome::Decided(_) => ScreenVerdict::Clear,
        DecisionOutcome::Failed => ScreenVerdict::Unread,
    }
}

/// What the run is told: the verdict when the usage is active, nothing
/// when it is only observed.
fn in_effect(mode: DecisionMode, verdict: ScreenVerdict) -> ScreenVerdict {
    match mode {
        DecisionMode::Active => verdict,
        DecisionMode::Observe | DecisionMode::Off => ScreenVerdict::Clear,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mimorya_agent::decision::{Decided, DecisionAnswer, EngineKind};

    use super::*;

    fn decided(yes: f64) -> DecisionOutcome {
        let answer = DecisionAnswer::Noul { yes };
        DecisionOutcome::Decided(Decided {
            engine: EngineKind::Jev,
            model: "jev-1.14.0".into(),
            latency_ms: 1,
            answers: BTreeMap::from([("instructions_pour_ia".to_string(), answer)]),
        })
    }

    #[test]
    fn instructions_or_a_failed_decision_count_when_active_only() {
        assert_eq!(verdict_of(&decided(0.9), 0.5), ScreenVerdict::Suspect);
        assert_eq!(verdict_of(&decided(0.1), 0.5), ScreenVerdict::Clear);
        assert_eq!(
            verdict_of(&DecisionOutcome::Failed, 0.5),
            ScreenVerdict::Unread
        );
        for verdict in [ScreenVerdict::Suspect, ScreenVerdict::Unread] {
            assert_eq!(in_effect(DecisionMode::Active, verdict), verdict);
            assert_eq!(
                in_effect(DecisionMode::Observe, verdict),
                ScreenVerdict::Clear
            );
        }
    }
}
