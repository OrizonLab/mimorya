//! What a case played for an evaluation is never given (ADR-104): it reads
//! only, with the tools that run without validation, and delegates nothing.
//! These ports answer should a call reach them anyway: no validation asked
//! for, no deliverable, no sub-agent.

use async_trait::async_trait;
use mimorya_agent::runtime::{
    ApprovalPlacement, ApprovalRequest, ApprovalStore, SubRun, SubRunRequest, SubRunStore,
};
use mimorya_core::tools::{Deliverable, DeliverableInput, DeliverableService, DeliverableStatus};
use uuid::Uuid;

/// Why a call that needs a validation does not run in an evaluation.
const NO_VALIDATION: &str =
    "Évaluation d’une compétence : aucune validation n’est demandée, l’action n’est pas exécutée.";

pub struct Withheld;

#[async_trait]
impl ApprovalStore for Withheld {
    async fn create(&self, _request: ApprovalRequest) -> anyhow::Result<ApprovalPlacement> {
        Ok(ApprovalPlacement::Unroutable(NO_VALIDATION.into()))
    }
}

#[async_trait]
impl DeliverableService for Withheld {
    async fn create(&self, _input: DeliverableInput) -> anyhow::Result<Deliverable> {
        anyhow::bail!("an evaluation produces no deliverable")
    }

    async fn get(&self, _id: Uuid) -> anyhow::Result<Option<Deliverable>> {
        Ok(None)
    }

    async fn set_status(&self, _id: Uuid, _status: DeliverableStatus) -> anyhow::Result<()> {
        anyhow::bail!("an evaluation produces no deliverable")
    }
}

#[async_trait]
impl SubRunStore for Withheld {
    async fn open(&self, _request: SubRunRequest) -> anyhow::Result<SubRun> {
        anyhow::bail!("an evaluation delegates nothing")
    }
}
