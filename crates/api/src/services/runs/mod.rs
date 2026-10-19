//! Agent runs started from conversations: opening a turn, executing it in the
//! background, recording approvals and resuming, persisting the outcome.

mod approval_decision;
mod context;
mod conversation_turn;
mod db_ports;
mod deliverables;
mod executor;
mod knowledge_ports;
mod outcome;
mod run_access;
mod service;

pub use approval_decision::DecisionInput;
pub use executor::RunExecutor;
pub use run_access::{AGENT_USE_WITHDRAWN, COMPANY_SUSPENDED};
pub use service::{DecisionResult, RunService, StartedTurn};
