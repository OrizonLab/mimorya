//! The agent run: loop, tool calls, verification, budgets, the ports it
//! uses to persist and report its progress, the lesson drawn from it and the
//! summary of its conversation's older messages.

pub mod answer_review;
pub mod budget;
pub mod context;
pub mod conversation_summary;
pub mod delegation;
mod document_store;
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

pub use budget::{BudgetLimits, DEFAULT_BUDGET, MISSION_STAGE_BUDGET, check_budget, share_left};
pub use context::{UserProfileContext, build_system_prompt, build_user_turn, french_date};
pub use conversation_summary::{
    FoldedMessage, SummaryAnswer, summarise_conversation, summary_message,
};
pub use delegation::{
    MAX_PER_TURN, SUB_AGENT_BUDGET, SubAgentProfile, SubRun, SubRunRequest, SubRunStore,
};
pub use document_store::{DocumentExcerpt, DocumentHit, DocumentPassage, DocumentStore};
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
