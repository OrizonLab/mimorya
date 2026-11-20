//! The agent run: loop, tool calls, verification, budgets, the ports it
//! uses to persist and report its progress, the lesson drawn from it and the
//! summary of its conversation's older messages.

mod action_guard;
mod advisor;
pub mod answer_review;
pub mod budget;
mod content_screen;
pub mod context;
pub mod conversation_summary;
mod decided_routing;
mod decision_costs;
pub mod delegation;
mod document_store;
pub mod escalation;
mod final_answer;
mod handoff;
pub mod lesson;
pub mod model_call;
pub mod ports;
mod recovery;
mod routing;
#[allow(clippy::module_inception)]
pub mod runtime;
pub mod skill_choice;
pub mod state;
pub mod supply;
pub mod tool_calls;
mod tool_refusals;
pub mod types;
mod unavailable;
pub mod verification;

pub use action_guard::{
    ActionGuard, GuardReview, GuardVerdict, GuardedCall, NoActionGuard, no_action_guard,
};
pub use budget::{BudgetLimits, DEFAULT_BUDGET, MISSION_STAGE_BUDGET, check_budget, share_left};
pub use content_screen::{
    ContentScreen, NoContentScreen, ScreenReview, ScreenVerdict, no_content_screen,
};
pub use context::{UserProfileContext, build_system_prompt, build_user_turn, french_date};
pub use conversation_summary::{
    FoldedMessage, SummaryAnswer, summarise_conversation, summary_message,
};
pub use delegation::{
    MAX_PER_TURN, SUB_AGENT_BUDGET, SubAgentProfile, SubRun, SubRunRequest, SubRunStore,
};
pub use document_store::{DocumentExcerpt, DocumentHit, DocumentPassage, DocumentStore};
pub use escalation::{Escalation, EscalationBudget, escalation_message};
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
pub use runtime::{ACCOUNT_INACTIVE, AgentRuntime};
pub use skill_choice::{
    MAX_ASKED_SKILLS, SkillsShown, chosen_skills, skill_choice_request, skills_section,
    with_skills, without_skills,
};
pub use state::{
    AdvisorState, EscalationState, PendingApproval, PendingCalls, RunCheckpoint, RunUsageTotals,
};
pub use supply::SupplyLimit;
pub use types::{
    ApprovalDecision, ApprovalDecisions, DEFAULT_TOOL_TIMEOUT, RunConfig, RunOutcome, RuntimeDeps,
    StartInput, VerificationConfig,
};
pub use unavailable::UnavailableConnectors;
pub use verification::VerificationReport;
