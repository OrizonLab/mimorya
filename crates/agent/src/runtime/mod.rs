//! The agent run: loop, tool calls, verification, budgets, the ports it
//! uses to persist and report its progress, and the lesson drawn from it.

pub mod answer_review;
pub mod budget;
pub mod context;
mod handoff;
pub mod lesson;
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

pub use budget::{BudgetLimits, DEFAULT_BUDGET, MISSION_STAGE_BUDGET, check_budget};
pub use context::{UserProfileContext, build_system_prompt, build_user_turn, french_date};
pub use handoff::NO_NOTE;
pub use lesson::{
    LessonAnswer, MAX_KNOWN_LESSONS, ProposedLesson, lesson_transcript, propose_lesson,
};
pub use ports::{
    AccessSource, ApprovalPlacement, ApprovalRequest, ApprovalStore, AuditSink, CurrentAccess,
    EngineAuditEntry, EnginePorts, MemoryHit, MemoryStore, NewMemory, RunEvent, RunEventSink,
    RunStore, SkillContent, SkillStore, SkillSummary, StepAccess, ToolInvocationLog,
    ToolInvocationRecord, UsageRecord, UsageSink,
};
pub use recovery::{InterruptedCall, MAX_RESUMPTIONS, unknown_effect};
pub use runtime::{ACCOUNT_INACTIVE, AgentRuntime, StartInput};
pub use state::{PendingApproval, PendingCalls, RunCheckpoint, RunUsageTotals};
pub use types::{
    ApprovalDecision, ApprovalDecisions, RunConfig, RunOutcome, RuntimeDeps, VerificationConfig,
};
pub use verification::VerificationReport;
