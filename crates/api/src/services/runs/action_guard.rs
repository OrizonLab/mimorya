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

use super::db_ports::RunScope;
use super::memory_guard::MAX_WAIT;
use crate::services::llm::{DecisionScope, LlmService, Models};

pub(super) struct DbActionGuard {
    pub db: Database,
    pub llm: Arc<LlmService>,
    pub models: Arc<Models>,
    pub scope: RunScope,
}

impl DbActionGuard {
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
        let scope = self.scope;
        let asked = DecisionScope {
            run_id: Some(scope.run_id),
            user_id: Some(scope.user_id),
        };
        let usage = [DecisionUsage::ActionGuard];
        let decisions = match (self.llm)
            .run_decisions(scope.tenant_id, asked, &self.models, &usage)
            .await
        {
            Ok(Some(decisions)) => decisions,
            Ok(None) => return GuardReview::default(),
            Err(error) => {
                tracing::warn!(error = %error, "Decisions unread: the action goes as decided");
                return GuardReview::default();
            }
        };
        let policy = decisions.policy(DecisionUsage::ActionGuard);
        if policy.mode == DecisionMode::Off {
            return GuardReview::default();
        }
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
        let verdict = match outcome {
            DecisionOutcome::Decided(decided) => {
                let signals = guard_signals(&decided, policy.threshold);
                if signals.is_empty() {
                    GuardVerdict::Clear
                } else {
                    GuardVerdict::Signals(signals.into_iter().map(Into::into).collect())
                }
            }
            DecisionOutcome::Failed => GuardVerdict::Unread,
        };
        tracing::info!(
            target: "mimorya::decision",
            usage = DecisionUsage::ActionGuard.as_str(),
            mode = policy.mode.as_str(),
            tool = call.tool,
            verdict = ?verdict,
            "Action read by the guard"
        );
        GuardReview {
            verdict: match policy.mode {
                DecisionMode::Active => verdict,
                DecisionMode::Observe | DecisionMode::Off => GuardVerdict::Clear,
            },
            counted,
        }
    }
}
