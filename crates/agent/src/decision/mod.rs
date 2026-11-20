//! The decision service (ADR-213): quick, typed judgements — a yes or no, a
//! choice among options, a level on a scale — asked of Jev or of the
//! company's main model, which can only make the platform more prudent and
//! fail safely to a value each usage writes in its code.

mod answer;
mod document_labels;
mod engine;
mod jev;
mod jev_wire;
mod main_model;
mod memory_guard;
mod question;
mod run;
mod service;
mod usage;

pub use answer::{Decided, DecisionAnswer, EngineKind};
pub use document_labels::{
    FIRST_PAGES, MAX_FIRST_TEXT_CHARS, classified as classified_labels, document_labels_request,
};
pub use engine::{DecisionEngine, EngineAnswer, EngineFailure};
pub use jev::{
    DEFAULT_JEV_MODEL, JEV_ENDPOINT, JEV_PROVIDER, JevClient, JevEngine, is_versioned_jev,
};
pub use main_model::MainModelEngine;
pub use memory_guard::{holds_sensitive_data, memory_guard_request};
pub use question::{
    DecisionQuestion, DecisionRequest, DecisionState, MAX_LEVELS, MAX_OPTIONS, MAX_STATE_CHARS,
    QuestionKind, TRUNCATED,
};
pub use run::RunDecisions;
pub use service::{Counted, DecisionCall, DecisionOutcome, DecisionRecorder, DecisionService};
pub use usage::{
    AVAILABLE_USAGES, DecisionMode, DecisionUsage, ENGINE_TEST, MAX_DECIDED_REQUEST_CHARS,
    UsagePolicy, usage_problem,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
