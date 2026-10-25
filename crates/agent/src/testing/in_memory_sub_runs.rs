//! The sub-agents' runs of the in-memory ports (ADR-098): each opened with
//! its own ports, reading the same knowledge as its parent, and opening
//! none itself. For tests only.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::in_memory_ports::InMemoryPorts;
use crate::runtime::{SubRun, SubRunRequest, SubRunStore};

/// A sub-agent's run, as its parent opened it.
#[derive(Clone)]
pub struct OpenedSubRun {
    pub request: SubRunRequest,
    pub run_id: Uuid,
    pub ports: Arc<InMemoryPorts>,
}

#[async_trait]
impl SubRunStore for InMemoryPorts {
    async fn open(&self, request: SubRunRequest) -> anyhow::Result<SubRun> {
        anyhow::ensure!(!self.is_sub_run, "a sub-agent opens no sub-agent");
        let ports = self.sub_run();
        let run_id = ports.run_id;
        self.state().sub_runs.push(OpenedSubRun {
            request,
            run_id,
            ports: ports.clone(),
        });
        Ok(SubRun {
            run_id,
            ports: ports.ports(),
        })
    }
}
