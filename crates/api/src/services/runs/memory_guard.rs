//! A memory the agent saves straight into a person's space, or a lesson it
//! proposes, read first by the company's decision service (ADR-213, D3):
//! whether it holds health, banking or sensitive personal data. Active, a
//! memory said so, or one the decision could not read, is saved as a
//! proposal its owner validates; a lesson said so is not kept (one it could
//! not read is proposed, as every lesson is). Observed, the decision is
//! journaled and nothing changes. A memory that already awaits a
//! validation asks nothing: the decision can only add one. Its engines are
//! asked within `MAX_WAIT` together, well within a tool call's own delay:
//! past it, the decision counts as unread, so that a slow engine never loses
//! the memory, and a call that answered is counted all the same.
//! The company's settings are read at each write: none read, the memory is
//! saved as asked, as without decisions.

use std::sync::Arc;
use std::time::Duration;

use mimorya_agent::decision::{
    DecisionMode, DecisionOutcome, DecisionUsage, holds_sensitive_data, memory_guard_request,
};
use mimorya_agent::runtime::{DEFAULT_TOOL_TIMEOUT, NewMemory};
use mimorya_core::memory::{MemoryScope, MemoryStatus};

use super::db_ports::RunScope;
use crate::services::llm::{DecisionScope, LlmService, Models};

/// How long a write, or an action (D5), waits for the decision's engines:
/// half a tool call's own delay, so that the call ends with the memory
/// saved.
pub(super) const MAX_WAIT: Duration = Duration::from_secs(DEFAULT_TOOL_TIMEOUT.as_secs() / 2);

/// What the decision said of a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Clear,
    Sensitive,
    /// No engine decided.
    Unread,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Verdict::Clear => "clear",
            Verdict::Sensitive => "sensitive",
            Verdict::Unread => "unread",
        }
    }
}

/// The status `memory` is saved with: a proposal when, active, the
/// decision holds it sensitive or cannot read it.
pub async fn memory_status(
    llm: &LlmService,
    models: &Arc<Models>,
    scope: RunScope,
    memory: &NewMemory,
) -> MemoryStatus {
    if memory.status != MemoryStatus::Active || memory.scope != MemoryScope::User {
        return memory.status;
    }
    match read(llm, models, scope, &memory.title, &memory.content).await {
        Some((DecisionMode::Active, Verdict::Sensitive | Verdict::Unread)) => {
            MemoryStatus::Proposed
        }
        _ => memory.status,
    }
}

/// Whether a lesson is kept: not when, active, the decision holds it
/// sensitive.
pub async fn lesson_kept(
    llm: &LlmService,
    models: &Arc<Models>,
    scope: RunScope,
    title: &str,
    text: &str,
) -> bool {
    !matches!(
        read(llm, models, scope, title, text).await,
        Some((DecisionMode::Active, Verdict::Sensitive))
    )
}

/// What the decision says of `title` and `text`, and how its usage is set;
/// `None` when off, or when the company's settings cannot be read.
async fn read(
    llm: &LlmService,
    models: &Arc<Models>,
    scope: RunScope,
    title: &str,
    text: &str,
) -> Option<(DecisionMode, Verdict)> {
    let asked = DecisionScope {
        run_id: Some(scope.run_id),
        user_id: Some(scope.user_id),
    };
    let decisions = llm
        .run_decisions(
            scope.tenant_id,
            asked,
            models,
            &[DecisionUsage::MemoryGuard],
        )
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Decisions unread: the memory is saved as asked");
            None
        })?;
    let policy = decisions.policy(DecisionUsage::MemoryGuard);
    if policy.mode == DecisionMode::Off {
        return None;
    }
    // Each engine within what is left of the wait: a call that answered is
    // counted, whatever follows.
    let request = memory_guard_request(title, text);
    let (outcome, _) = decisions
        .service
        .decide_within(&request, Some(MAX_WAIT))
        .await;
    let verdict = match outcome {
        DecisionOutcome::Decided(decided) if holds_sensitive_data(&decided, policy.threshold) => {
            Verdict::Sensitive
        }
        DecisionOutcome::Decided(_) => Verdict::Clear,
        // No engine decided, or not in time.
        DecisionOutcome::Failed => Verdict::Unread,
    };
    tracing::info!(
        target: "mimorya::decision",
        usage = DecisionUsage::MemoryGuard.as_str(),
        mode = policy.mode.as_str(),
        verdict = verdict.as_str(),
        "Memory read for sensitive data"
    );
    Some((policy.mode, verdict))
}
