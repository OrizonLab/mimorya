//! The guard of the actions (ADR-213, D5) and the detection of
//! instructions in external content (D6) as the tests set them: one
//! verdict for every call or content, clear by default, and the tools each
//! was asked of.

use async_trait::async_trait;

use super::in_memory_ports::InMemoryPorts;
use crate::runtime::{
    ActionGuard, ContentScreen, GuardReview, GuardVerdict, GuardedCall, ScreenReview, ScreenVerdict,
};

impl InMemoryPorts {
    /// The guard says `verdict` of every call from now on.
    pub fn guard_says(&self, verdict: GuardVerdict) {
        self.state().guard_verdict = verdict;
    }

    /// The tools the guard was asked of, in order.
    pub fn guarded(&self) -> Vec<String> {
        self.state().guarded.clone()
    }

    /// The detection says `verdict` of every content from now on.
    pub fn screen_says(&self, verdict: ScreenVerdict) {
        self.state().screen_verdict = verdict;
    }

    /// The tools whose results were screened, in order.
    pub fn screened(&self) -> Vec<String> {
        self.state().screened.clone()
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

#[async_trait]
impl ContentScreen for InMemoryPorts {
    async fn screen(&self, tool: &str, _content: &str) -> ScreenReview {
        let mut state = self.state();
        state.screened.push(tool.to_string());
        ScreenReview {
            verdict: state.screen_verdict,
            counted: Vec::new(),
        }
    }
}
