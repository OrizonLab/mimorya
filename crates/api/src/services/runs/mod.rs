//! Agent runs started from conversations: opening a turn, executing it in the
//! background, recording approvals and resuming, persisting the outcome.

mod approval_decision;
mod approval_delegation;
mod approval_notice;
mod approval_routing;
mod background_usage;
mod context;
mod conversation_turn;
mod db_ports;
mod deliverables;
mod document_ports;
mod events;
mod executor;
mod expiry;
mod fence;
mod history;
mod knowledge_ports;
mod leases;
mod lesson_capture;
mod outcome;
mod queue;
mod reads;
mod recovery;
mod run_access;
mod service;
mod sub_runs;
mod summaries;
mod turn_start;

pub use approval_decision::{DecisionInput, NOT_FOUND as APPROVAL_NOT_FOUND};
pub use approval_delegation::{
    AbsenceInput, Delegations, MAX_ABSENCE, absence_period, declare_absence, delegations,
    end_absence, named_delegate,
};
pub use approval_notice::{ApprovalNotifier, Recipient, recipient};
pub use approval_routing::{can_decide, can_withdraw, may_decide_for, shown_to, stands_in};
/// What an evaluation of a skill plays its cases with (ADR-104).
pub(crate) use context::{run_tools, user_profile};
pub use conversation_turn::{TITLE_LENGTH, TurnSender, tenant_autonomy_level};
pub use db_ports::RunScope;
pub use events::{append_in, record_run_event};
pub use executor::RunExecutor;
pub use leases::LeaseTiming;
pub use lesson_capture::LessonCapture;
pub use queue::{QUEUED_REQUEST_NOT_FOUND, QUEUED_TOO_LONG};
pub(crate) use reads::Reader;
pub use recovery::{
    INTERRUPTED_TOO_LONG, NOT_AN_EXECUTOR, STOPPED_BEFORE_ITS_FIRST_STEP, SUB_AGENT_INTERRUPTED,
};
pub use run_access::{AGENT_USE_WITHDRAWN, COMPANY_SUSPENDED};
pub(crate) use run_access::{step_access, stopped_by};
pub use service::{DecisionResult, RunService};
pub use sub_runs::{SUB_AGENT_STOPPED, announce, end_sub_runs};
pub use summaries::ConversationSummaries;
pub use turn_start::{COMPANY_NOT_ACTIVE, GatewayThread, StartedTurn};
