//! Where a run opens its sub-agents' runs (ADR-098): recorded beside it, each
//! with the ports it reports through. A sub-agent's own store opens none:
//! sub-agents do not delegate further.

use async_trait::async_trait;
use uuid::Uuid;

use super::profile::SubAgentProfile;
use crate::runtime::budget::BudgetLimits;
use crate::runtime::ports::EnginePorts;

/// A child run to open.
#[derive(Debug, Clone, PartialEq)]
pub struct SubRunRequest {
    pub profile: SubAgentProfile,
    /// The task, as the parent gave it.
    pub objective: String,
    /// Its share of what is left of the parent's budget.
    pub budget: BudgetLimits,
    /// The permissions the run may use at most, as its parent's ceiling
    /// says (`None`: the user's, the agent ceiling applied).
    pub allowed_permissions: Option<Vec<String>>,
}

/// A child run, opened.
pub struct SubRun {
    pub run_id: Uuid,
    pub ports: EnginePorts,
}

#[async_trait]
pub trait SubRunStore: Send + Sync {
    /// Records a child run of this run, running, and returns its ports;
    /// fails for a run that is itself a sub-agent.
    async fn open(&self, request: SubRunRequest) -> anyhow::Result<SubRun>;
}
