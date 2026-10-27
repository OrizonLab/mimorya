//! What a case played for an evaluation is never given (ADR-104): it reads
//! only, with the tools that run without validation, and delegates nothing.
//! These ports answer should a call reach them anyway: no validation asked
//! for, no deliverable, no sub-agent, no account a person linked (ADR-117):
//! a case is no one's request, so no one's mailbox is read.

use async_trait::async_trait;
use mimorya_agent::runtime::{
    ApprovalPlacement, ApprovalRequest, ApprovalStore, SubRun, SubRunRequest, SubRunStore,
};
use mimorya_core::tools::{
    AccountAccessError, Deliverable, DeliverableInput, DeliverableService, DeliverableStatus,
    LinkedAccounts,
};
use uuid::Uuid;

/// Why a linked account is not read in an evaluation.
pub const NO_LINKED_ACCOUNT: &str =
    "Évaluation d’une compétence : les comptes reliés par chacun ne sont pas lus.";

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

#[async_trait]
impl LinkedAccounts for Withheld {
    async fn access_token(&self, _connector_type: &str) -> Result<String, AccountAccessError> {
        Err(AccountAccessError::Unavailable(NO_LINKED_ACCOUNT.into()))
    }
}
