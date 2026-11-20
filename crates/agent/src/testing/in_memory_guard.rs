//! The guard of the actions as the tests set it (ADR-213, D5): one verdict
//! for every call, clear by default, and the tools it was asked of.

use async_trait::async_trait;

use super::in_memory_ports::InMemoryPorts;
use crate::runtime::{ActionGuard, GuardReview, GuardVerdict, GuardedCall};

impl InMemoryPorts {
    /// The guard says `verdict` of every call from now on.
    pub fn guard_says(&self, verdict: GuardVerdict) {
        self.state().guard_verdict = verdict;
    }

    /// The tools the guard was asked of, in order.
    pub fn guarded(&self) -> Vec<String> {
        self.state().guarded.clone()
    }
}

#[async_trait]
impl ActionGuard for InMemoryPorts {
    async fn review(&self, call: &GuardedCall<'_>) -> GuardReview {
        let mut state = self.state();
        state.guarded.push(call.tool.to_string());
        GuardReview {
            verdict: state.guard_verdict.clone(),
            counted: Vec::new(),
        }
    }
}
