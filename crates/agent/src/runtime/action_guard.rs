//! The company's guard of the actions (ADR-213, D5), a port the platform
//! provides: asked of each call with a side effect the policy enforcement
//! point lets run or puts to a person's validation, before either. It can
//! only add: an action allowed alone that it signals, or cannot read, goes
//! to a person's validation; an action already put to validation keeps it,
//! the signals told to whoever decides. A call denied, one that reads or
//! drafts only, a delegation and a call a person validated are never
//! asked. Without a guard set, nothing is asked.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use serde_json::Value;

use crate::decision::Counted;
use crate::tools::platform::DELEGATE_TASK;
use crate::tools::{RegisteredTool, ToolCallEvaluation};

/// A call about to run or to be put to validation, as the guard reads it.
#[derive(Debug, Clone, Copy)]
pub struct GuardedCall<'a> {
    pub tool: &'a str,
    pub title: &'a str,
    /// The action as its tool describes it, if it does.
    pub action: Option<&'a str>,
    pub input: &'a Value,
}

/// What the guard said of a call.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GuardVerdict {
    /// Nothing to say: not set, observed only, or nothing signalled.
    #[default]
    Clear,
    /// What calls for a person, in French.
    Signals(Vec<String>),
    /// No engine decided: the usage's prudent value is a validation.
    Unread,
}

/// The verdict, and what its decision cost (counted in the run).
#[derive(Debug, Default)]
pub struct GuardReview {
    pub verdict: GuardVerdict,
    pub counted: Vec<Counted>,
}

#[async_trait]
pub trait ActionGuard: Send + Sync {
    async fn review(&self, call: &GuardedCall<'_>) -> GuardReview;
}

/// No guard: every action goes as the policy decided.
pub struct NoActionGuard;

#[async_trait]
impl ActionGuard for NoActionGuard {
    async fn review(&self, _call: &GuardedCall<'_>) -> GuardReview {
        GuardReview::default()
    }
}

pub fn no_action_guard() -> Arc<dyn ActionGuard> {
    Arc::new(NoActionGuard)
}

/// The tool whose call `evaluation` lets run or puts to validation, if the
/// guard is asked of it: one with a side effect, not a delegation.
pub(super) fn guarded(evaluation: &ToolCallEvaluation) -> Option<&RegisteredTool> {
    let tool = match evaluation {
        ToolCallEvaluation::Allow { tool, .. }
        | ToolCallEvaluation::RequireApproval { tool, .. } => tool,
        ToolCallEvaluation::Deny { .. } => return None,
    };
    let acts = matches!(
        tool.info.risk,
        ToolRisk::Write | ToolRisk::External | ToolRisk::Irreversible
    );
    (acts && tool.info.name != DELEGATE_TASK).then_some(tool)
}

/// `evaluation` once the guard said `verdict`: a validation added to an
/// allowed call it signals or cannot read, its signals told with one
/// already asked; never anything removed.
pub(super) fn guarded_evaluation(
    evaluation: ToolCallEvaluation,
    verdict: &GuardVerdict,
) -> ToolCallEvaluation {
    match (evaluation, verdict) {
        (evaluation, GuardVerdict::Clear) => evaluation,
        (ToolCallEvaluation::Allow { tool, input }, GuardVerdict::Signals(signals)) => {
            ToolCallEvaluation::RequireApproval {
                tool,
                input,
                reason: format!(
                    "Validation demandée par le service de décision : {}",
                    signals.join(", ")
                ),
            }
        }
        (ToolCallEvaluation::Allow { tool, input }, GuardVerdict::Unread) => {
            ToolCallEvaluation::RequireApproval {
                tool,
                input,
                reason: "Validation demandée : le service de décision n’a pas pu se prononcer"
                    .into(),
            }
        }
        (
            ToolCallEvaluation::RequireApproval {
                tool,
                input,
                reason,
            },
            GuardVerdict::Signals(signals),
        ) => ToolCallEvaluation::RequireApproval {
            tool,
            input,
            reason: format!(
                "{reason} · Signalé par le service de décision : {}",
                signals.join(", ")
            ),
        },
        (evaluation, _) => evaluation,
    }
}
