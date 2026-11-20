//! The decisions a run may ask (ADR-213): the company's service, and how
//! each usage is set. A usage off, or one whose step is not delivered,
//! asks nothing.

use std::sync::Arc;

use super::service::DecisionService;
use super::usage::{DecisionMode, DecisionUsage, UsagePolicy};

pub struct RunDecisions {
    pub service: Arc<DecisionService>,
    policies: Vec<(DecisionUsage, UsagePolicy)>,
}

impl RunDecisions {
    pub fn new(service: Arc<DecisionService>, policies: Vec<(DecisionUsage, UsagePolicy)>) -> Self {
        Self { service, policies }
    }

    /// How `usage` is set: off unless set and delivered.
    pub fn policy(&self, usage: DecisionUsage) -> UsagePolicy {
        let off = UsagePolicy {
            mode: DecisionMode::Off,
            threshold: 0.5,
        };
        if !usage.available() {
            return off;
        }
        self.policies
            .iter()
            .find(|(set, _)| *set == usage)
            .map_or(off, |(_, policy)| *policy)
    }
}
