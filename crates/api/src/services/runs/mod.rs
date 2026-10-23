//! Agent runs started from conversations: opening a turn, executing it in the
//! background, recording approvals and resuming, persisting the outcome.

mod approval_decision;
mod approval_delegation;
mod approval_notice;
mod approval_routing;
mod context;
mod conversation_turn;
mod db_ports;
mod deliverables;
mod events;
mod executor;
mod expiry;
mod knowledge_ports;
mod leases;
mod lesson_capture;
mod outcome;
mod queue;
mod recovery;
mod run_access;
mod service;

pub use approval_decision::DecisionInput;
pub use approval_delegation::{
    AbsenceInput, Delegations, MAX_ABSENCE, absence_period, declare_absence, delegations,
    end_absence, named_delegate,
};
pub use approval_notice::ApprovalNotifier;
pub use approval_routing::{can_decide, can_withdraw, shown_to};
pub use conversation_turn::TurnSender;
pub use events::record_run_event;
pub use executor::RunExecutor;
pub use leases::LeaseTiming;
pub use lesson_capture::LessonCapture;
pub use queue::{QUEUED_REQUEST_NOT_FOUND, QUEUED_TOO_LONG};
pub use recovery::{
    INTERRUPTED_TOO_LONG, LOCK_HELD_ELSEWHERE, NOT_AN_EXECUTOR, STOPPED_BEFORE_ITS_FIRST_STEP,
};
pub use run_access::{AGENT_USE_WITHDRAWN, COMPANY_SUSPENDED};
pub use service::{DecisionResult, RunService, StartedTurn};
