//! The rights a run reads before each step (ADR-035): its user's access as it
//! stands now, through the same service as requests. The company's rules come
//! from its cache, invalidated by every administration change; the account,
//! its assignments and attributes from the database. The run goes on only
//! while the account is active and may still use the agent, as when it
//! started (`services::runs::service`), and while its company is active. A
//! company is suspended in the database only (no route nor command does it
//! yet): the run learns it once the company's cached rules expire (30 s at
//! most).

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, AccessSource, CurrentAccess, StepAccess};
use uuid::Uuid;

use crate::services::access::AccessService;

/// Why a run stops once its user lost the right to use the agent.
pub const AGENT_USE_WITHDRAWN: &str =
    "Arrêt : l’utilisateur n’a plus le droit d’utiliser l’assistant";
/// Why a run stops once its company is suspended.
pub const COMPANY_SUSPENDED: &str = "Arrêt : l’entreprise est suspendue";

pub struct RunAccess {
    access: Arc<AccessService>,
    tenant_id: Uuid,
    user_id: Uuid,
}

impl RunAccess {
    pub fn new(access: Arc<AccessService>, tenant_id: Uuid, user_id: Uuid) -> Self {
        Self {
            access,
            tenant_id,
            user_id,
        }
    }
}

#[async_trait]
impl AccessSource for RunAccess {
    async fn current(&self) -> anyhow::Result<StepAccess> {
        Ok(
            match self
                .access
                .active_context(self.tenant_id, self.user_id)
                .await?
            {
                None => StepAccess::Ended(ACCOUNT_INACTIVE.into()),
                Some(context) if context.tenant.status != "active" => {
                    StepAccess::Ended(COMPANY_SUSPENDED.into())
                }
                Some(context) if !context.has("agent:use") => {
                    StepAccess::Ended(AGENT_USE_WITHDRAWN.into())
                }
                Some(context) => StepAccess::Granted(CurrentAccess {
                    engine: context.engine,
                    subject: context.subject,
                }),
            },
        )
    }
}
