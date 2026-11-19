//! The decision service (ADR-213): quick, typed judgements — a yes or no, a
//! choice among options, a level on a scale — asked of Jev or of the
//! company's main model, which can only make the platform more prudent and
//! fail safely to a value each usage writes in its code.

mod answer;
mod engine;
mod jev;
mod jev_wire;
mod main_model;
mod question;
mod service;
mod usage;

pub use answer::{Decided, DecisionAnswer, EngineKind};
pub use engine::{DecisionEngine, EngineAnswer, EngineFailure};
pub use jev::{DEFAULT_JEV_MODEL, JEV_ENDPOINT, JEV_PROVIDER, JevEngine, is_versioned_jev};
pub use main_model::MainModelEngine;
pub use question::{
    DecisionQuestion, DecisionRequest, DecisionState, MAX_LEVELS, MAX_OPTIONS, MAX_STATE_CHARS,
    QuestionKind, TRUNCATED,
};
pub use service::{DecisionCall, DecisionOutcome, DecisionRecorder, DecisionService};
pub use usage::{
    AVAILABLE_USAGES, DecisionMode, DecisionUsage, ENGINE_TEST, UsagePolicy, usage_problem,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
