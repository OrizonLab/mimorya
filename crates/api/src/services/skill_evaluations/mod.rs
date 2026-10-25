//! Evaluations of a skill's versions (ADR-104): asked for from the
//! administration (`services::admin::skill_evaluations`), played by a
//! process that executes runs, one at a time (`pass.rs`), case by case
//! (`play.rs`, `case.rs`) with ports of their own (`ports/`), and read back
//! (`view.rs`).

mod access;
mod case;
mod pass;
mod play;
mod ports;
mod view;

pub use case::{CASE_BUDGET, StoredCase};
pub use pass::{EVALUATION_SWEEP, play_pending_evaluations, wake};
pub use ports::ToolTrace;
pub use view::{EvaluationRead, evaluation, evaluations_of, last_evaluation};
