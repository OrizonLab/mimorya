//! Evaluations of a skill's versions (ADR-104): asked for from the
//! administration (`services::admin::skill_evaluations`), played by a
//! process that executes runs, one at a time (`pass.rs`), case by case
//! (`play.rs`, `case.rs`) with ports of their own (`ports/`), within what
//! the company's evaluations may cost over thirty days (`spending.rs`,
//! ADR-154), and read back (`view.rs`).

mod access;
mod case;
mod pass;
mod play;
mod ports;
mod spending;
mod view;

pub use case::StoredCase;
pub use pass::{EVALUATION_SWEEP, play_pending_evaluations, wake};
pub use ports::NO_LINKED_ACCOUNT;
pub use spending::{
    MAX_EVALUATION_LIMIT_MICROS, MIN_EVALUATION_LIMIT_MICROS, PLATFORM_EVALUATION_LIMIT_MICROS,
    SPENDING_WINDOW_DAYS, Spending, checked_limit, count_spent, spending,
};
pub use view::{EvaluationRead, evaluation, evaluations_of, last_evaluation, shown_to};
