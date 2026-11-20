//! The company's guard of the actions (ADR-213, D5), asked by the engine
//! of each call with a side effect, before it runs or is put to a person's
//! validation. Its settings are read at each call, so that a change holds
//! at once, even in a run resumed after a validation. It sends the tool,
//! the action as it describes it, its parameters and the served person's
//! own e-mail domain, nothing else. Active, the signals it raises, or an
//! engine that cannot decide within the wait, call for a person; observed,
//! it is asked, counted and journaled, without effect; off, or its settings
//! unread, nothing is asked. Its cost is counted in the run.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::decision::{
    DecisionMode, DecisionOutcome, DecisionUsage, GuardedAction, action_guard_request,
    guard_signals,
};
use mimorya_agent::runtime::{ActionGuard, GuardReview, GuardVerdict, GuardedCall};
use mimorya_db::Database;
use mimorya_db::repositories::users::get_user;

use super::db_ports::{DbPorts, RunScope};
use super::usage_decisions::{MAX_WAIT, usage_decisions};
use crate::services::llm::{LlmService, Models};

pub(super) struct DbActionGuard {
    db: Database,
    llm: Arc<LlmService>,
    models: Arc<Models>,
    scope: RunScope,
}

impl DbActionGuard {
    /// The guard of the run `ports` serve.
    pub(super) fn of(ports: &DbPorts) -> Self {
        Self {
            db: ports.db.clone(),
            llm: ports.llm.clone(),
            models: ports.models.clone(),
            scope: ports.scope,
        }
    }

    /// The served person's own e-mail domain, to tell an external recipient.
    async fn internal_domain(&self) -> Option<String> {
        let mut tx = self.db.begin(self.scope.tenant_id).await.ok()?;
        let user = get_user(&mut tx, self.scope.user_id).await.ok()??;
        tx.commit().await.ok()?;
        let (_, domain) = user.email.rsplit_once('@')?;
        Some(domain.to_string())
    }
}

#[async_trait]
impl ActionGuard for DbActionGuard {
    async fn review(&self, call: &GuardedCall<'_>) -> GuardReview {
        let usage = DecisionUsage::ActionGuard;
        let Some((decisions, policy)) =
            usage_decisions(&self.llm, &self.models, self.scope, usage).await
        else {
            return GuardReview::default();
        };
        let domain = self.internal_domain().await;
        let parameters = call.input.to_string();
        let request = action_guard_request(&GuardedAction {
            tool: call.tool,
            title: call.title,
            action: call.action,
            parameters: &parameters,
            internal_domain: domain.as_deref(),
        });
        // Each engine within what is left of the wait: a call that answered
        // is counted, whatever follows.
        let (outcome, counted) = decisions
            .service
            .decide_within(&request, Some(MAX_WAIT))
            .await;
        let verdict = verdict_of(&outcome, policy.threshold);
        tracing::info!(
            target: "mimorya::decision",
            usage = DecisionUsage::ActionGuard.as_str(),
            mode = policy.mode.as_str(),
            tool = call.tool,
            verdict = ?verdict,
            "Action read by the guard"
        );
        GuardReview {
            verdict: in_effect(policy.mode, verdict),
            counted,
        }
    }
}

/// What the decision says: its signals at `threshold`, or, no engine
/// having decided, the usage's prudent value.
fn verdict_of(outcome: &DecisionOutcome, threshold: f64) -> GuardVerdict {
    match outcome {
        DecisionOutcome::Decided(decided) => {
            let signals = guard_signals(decided, threshold);
            if signals.is_empty() {
                GuardVerdict::Clear
            } else {
                GuardVerdict::Signals(signals.into_iter().map(Into::into).collect())
            }
        }
        DecisionOutcome::Failed => GuardVerdict::Unread,
    }
}

/// What the run is told: the verdict when the usage is active, nothing
/// when it is only observed.
fn in_effect(mode: DecisionMode, verdict: GuardVerdict) -> GuardVerdict {
    match mode {
        DecisionMode::Active => verdict,
        DecisionMode::Observe | DecisionMode::Off => GuardVerdict::Clear,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mimorya_agent::decision::{Decided, DecisionAnswer, EngineKind};

    use super::*;

    fn decided(yes: f64) -> DecisionOutcome {
        let mut answers: BTreeMap<String, DecisionAnswer> = [
            "destinataire_externe",
            "montant_eleve",
            "donnees_personnelles",
            "irreversible",
        ]
        .into_iter()
        .map(|name| (name.to_string(), DecisionAnswer::Noul { yes }))
        .collect();
        answers.insert(
            "risque".into(),
            DecisionAnswer::Score {
                score: 0.0,
                confidence: 0.9,
                probabilities: vec![0.9, 0.1, 0.0, 0.0],
            },
        );
        DecisionOutcome::Decided(Decided {
            engine: EngineKind::Jev,
            model: "jev-1.14.0".into(),
            latency_ms: 1,
            answers,
        })
    }

    #[test]
    fn a_failed_decision_calls_for_a_person_when_active_only() {
        assert_eq!(
            verdict_of(&DecisionOutcome::Failed, 0.5),
            GuardVerdict::Unread
        );
        assert_eq!(
            in_effect(DecisionMode::Active, GuardVerdict::Unread),
            GuardVerdict::Unread
        );
        assert_eq!(
            in_effect(DecisionMode::Observe, GuardVerdict::Unread),
            GuardVerdict::Clear
        );
    }

    #[test]
    fn signals_at_the_threshold_are_told_nothing_else() {
        assert_eq!(verdict_of(&decided(0.1), 0.5), GuardVerdict::Clear);
        let GuardVerdict::Signals(signals) = verdict_of(&decided(0.9), 0.5) else {
            panic!("signals expected")
        };
        assert_eq!(signals.len(), 4);
        assert!(signals.contains(&"destinataire externe".to_string()));
        let signalled = GuardVerdict::Signals(signals);
        assert_eq!(
            in_effect(DecisionMode::Observe, signalled),
            GuardVerdict::Clear
        );
    }
}
