//! Agent runs started from conversations: opening a turn, executing it in the
//! background, recording approvals and resuming, persisting the outcome.

mod approval_decision;
mod approval_routing;
mod context;
mod conversation_turn;
mod db_ports;
mod deliverables;
mod events;
mod executor;
mod knowledge_ports;
mod outcome;
mod recovery;
mod run_access;
mod service;

pub use approval_decision::DecisionInput;
pub use approval_routing::can_decide;
pub use events::record_run_event;
pub use executor::RunExecutor;
pub use recovery::{INTERRUPTED_TOO_LONG, LOCK_HELD_ELSEWHERE, STOPPED_BEFORE_ITS_FIRST_STEP};
pub use run_access::{AGENT_USE_WITHDRAWN, COMPANY_SUSPENDED};
pub use service::{DecisionResult, RunService, StartedTurn};
