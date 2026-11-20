//! The decisions a run asks as it goes (ADR-213, D3, D5, D6): the company's
//! settings of one usage read at each ask, so that a change holds at once,
//! even in a run resumed after a validation; none built while the usage is
//! off or its settings cannot be read, as without decisions. Their engines
//! are asked within `MAX_WAIT` together.

use std::sync::Arc;
use std::time::Duration;

use mimorya_agent::decision::{DecisionMode, DecisionUsage, RunDecisions, UsagePolicy};
use mimorya_agent::runtime::DEFAULT_TOOL_TIMEOUT;

use super::db_ports::RunScope;
use crate::services::llm::{DecisionScope, LlmService, Models};

/// How long an ask waits for the decision's engines: half a tool call's own
/// delay, so that a memory is saved, an action decided or a result read
/// within the call that asked.
pub(super) const MAX_WAIT: Duration = Duration::from_secs(DEFAULT_TOOL_TIMEOUT.as_secs() / 2);

/// The decisions of `usage` for the run of `scope`, and how it is set;
/// `None` when it is off, or its settings cannot be read.
pub(super) async fn usage_decisions(
    llm: &LlmService,
    models: &Arc<Models>,
    scope: RunScope,
    usage: DecisionUsage,
) -> Option<(Arc<RunDecisions>, UsagePolicy)> {
    let asked = DecisionScope {
        run_id: Some(scope.run_id),
        user_id: Some(scope.user_id),
    };
    let decisions = match llm
        .run_decisions(scope.tenant_id, asked, models, &[usage])
        .await
    {
        Ok(decisions) => decisions?,
        Err(error) => {
            tracing::warn!(
                usage = usage.as_str(),
                error = %error,
                "Decisions unread: as without decisions"
            );
            return None;
        }
    };
    let policy = decisions.policy(usage);
    (policy.mode != DecisionMode::Off).then_some((decisions, policy))
}
