//! The agent run: loop, tool calls, verification, budgets, and the ports it
//! uses to persist and report its progress.

pub mod answer_review;
pub mod budget;
pub mod context;
pub mod model_call;
pub mod ports;
mod recovery;
#[allow(clippy::module_inception)]
pub mod runtime;
pub mod state;
pub mod tool_calls;
mod tool_refusals;
pub mod types;
mod unavailable;
pub mod verification;

pub use budget::{BudgetLimits, DEFAULT_BUDGET, check_budget};
pub use context::{UserProfileContext, build_system_prompt, build_user_turn, french_date};
pub use ports::{
    AccessSource, ApprovalRequest, ApprovalStore, AuditSink, CurrentAccess, EngineAuditEntry,
    EnginePorts, MemoryHit, MemoryStore, NewMemory, RunEvent, RunEventSink, RunStore, SkillContent,
    SkillStore, SkillSummary, StepAccess, ToolInvocationLog, ToolInvocationRecord, UsageRecord,
    UsageSink,
};
pub use recovery::MAX_RESUMPTIONS;
pub use runtime::{ACCOUNT_INACTIVE, AgentRuntime, StartInput};
pub use state::{PendingApproval, PendingCalls, RunCheckpoint, RunUsageTotals};
pub use types::{
    ApprovalDecision, ApprovalDecisions, RunConfig, RunOutcome, RuntimeDeps, VerificationConfig,
};
pub use verification::VerificationReport;
