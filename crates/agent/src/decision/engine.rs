//! An engine of decisions (ADR-213): Jev or the company's main model, each
//! answering the same typed questions; and why one failed, which the
//! service reads to fall back, never shows as such.

use std::collections::BTreeMap;

use async_trait::async_trait;

use super::answer::{DecisionAnswer, EngineKind};
use super::question::DecisionRequest;
use crate::llm::LlmUsage;

/// What an engine answered, before it is checked against the questions.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineAnswer {
    /// As usage is recorded: `typesafe` for Jev, else the model's provider.
    pub provider: String,
    /// The model that answered (Jev's versioned identifier).
    pub model: String,
    pub answers: BTreeMap<String, DecisionAnswer>,
    pub usage: LlmUsage,
}

/// Why an engine gave no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineFailure {
    /// Unreachable, overloaded or failing (5xx, 429, 529, the network).
    Unavailable,
    /// It refused the request (its key, the request's size or shape).
    Refused,
    /// Its answer is not the one asked for (JSON, types, values).
    Invalid,
    /// It took longer than allowed.
    TimedOut,
    /// The operator's supply of the company is spent (ADR-208).
    SupplySpent,
}

impl EngineFailure {
    pub fn as_str(self) -> &'static str {
        match self {
            EngineFailure::Unavailable => "unavailable",
            EngineFailure::Refused => "refused",
            EngineFailure::Invalid => "invalid",
            EngineFailure::TimedOut => "timed_out",
            EngineFailure::SupplySpent => "supply_spent",
        }
    }
}

#[async_trait]
pub trait DecisionEngine: Send + Sync {
    fn kind(&self) -> EngineKind;

    /// The model it asks (Jev's pinned version, or the main model).
    fn model(&self) -> &str;

    async fn answer(&self, request: &DecisionRequest) -> Result<EngineAnswer, EngineFailure>;
}
