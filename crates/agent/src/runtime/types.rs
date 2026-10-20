//! Public types of the agent runtime: dependencies, run configuration and outcomes.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mimorya_core::authz::Actor;
use mimorya_core::autonomy::AutonomyPolicy;
use uuid::Uuid;

use super::budget::BudgetLimits;
use super::ports::{AccessSource, EnginePorts};
use super::state::{PendingApproval, RunCheckpoint};
use super::verification::VerificationReport;
use crate::llm::{Effort, ModelRouter, PriceTable};
use crate::tools::ToolRegistry;

#[derive(Clone)]
pub struct RuntimeDeps {
    pub router: ModelRouter,
    /// The user's rights, read again before each step.
    pub access: Arc<dyn AccessSource>,
    pub registry: Arc<ToolRegistry>,
    pub ports: EnginePorts,
    pub prices: Arc<PriceTable>,
    /// Retries of retryable provider errors.
    pub max_provider_retries: u32,
    pub retry_delay: Duration,
    pub tool_timeout: Duration,
}

impl RuntimeDeps {
    pub fn new(
        router: ModelRouter,
        access: Arc<dyn AccessSource>,
        registry: Arc<ToolRegistry>,
        ports: EnginePorts,
        prices: Arc<PriceTable>,
    ) -> Self {
        Self {
            router,
            access,
            registry,
            ports,
            prices,
            max_provider_retries: 2,
            retry_delay: Duration::from_secs(1),
            tool_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationConfig {
    /// Model-based review of answers against explicit success criteria.
    pub llm_judge: bool,
    pub max_correction_attempts: u32,
}

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub run_id: Uuid,
    pub actor: Actor,
    pub objective: String,
    pub autonomy: AutonomyPolicy,
    pub budget: BudgetLimits,
    pub success_criteria: Vec<String>,
    pub verification: VerificationConfig,
    pub effort: Option<Effort>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalDecision {
    pub approved: bool,
    pub comment: Option<String>,
    /// The request expired before anyone decided: not approved, nor refused
    /// by the user.
    pub expired: bool,
}

impl ApprovalDecision {
    pub fn approved() -> Self {
        Self {
            approved: true,
            comment: None,
            expired: false,
        }
    }

    pub fn rejected(comment: Option<String>) -> Self {
        Self {
            approved: false,
            comment,
            expired: false,
        }
    }

    pub fn expired() -> Self {
        Self {
            approved: false,
            comment: None,
            expired: true,
        }
    }
}

pub type ApprovalDecisions = HashMap<Uuid, ApprovalDecision>;

#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    Completed {
        answer: String,
        verification: VerificationReport,
        checkpoint: RunCheckpoint,
    },
    AwaitingApproval {
        approvals: Vec<PendingApproval>,
        checkpoint: RunCheckpoint,
    },
    Failed {
        error: String,
        checkpoint: RunCheckpoint,
    },
}

impl RunOutcome {
    /// `completed`, `awaiting_approval` or `failed`.
    pub fn status(&self) -> &'static str {
        match self {
            Self::Completed { .. } => "completed",
            Self::AwaitingApproval { .. } => "awaiting_approval",
            Self::Failed { .. } => "failed",
        }
    }

    pub fn checkpoint(&self) -> &RunCheckpoint {
        match self {
            Self::Completed { checkpoint, .. }
            | Self::AwaitingApproval { checkpoint, .. }
            | Self::Failed { checkpoint, .. } => checkpoint,
        }
    }
}
