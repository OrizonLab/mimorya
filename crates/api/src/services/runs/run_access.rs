//! The rights a run reads before each step (ADR-035): its user's access as it
//! stands now, through the same service as requests. The company's rules come
//! from its cache, invalidated by every administration change; the account,
//! its assignments and attributes from the database. The run goes on only
//! while the account is active and may still use the agent, as when it
//! started (`services::runs::service`), and while its company is active. A
//! company is suspended in the database only (no route nor command does it
//! yet): the run learns it once the company's cached rules expire (30 s at
//! most). Before all that, the execution must still hold its lease
//! (ADR-086): one that lost it stops, calling no model and no tool.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, AccessSource, CurrentAccess, StepAccess};
use uuid::Uuid;

use super::fence::LeaseFence;
use crate::services::access::{AccessContext, AccessService};

/// Why a run stops once its user lost the right to use the agent.
pub const AGENT_USE_WITHDRAWN: &str =
    "Arrêt : l’utilisateur n’a plus le droit d’utiliser l’assistant";
/// Why a run stops once its company is suspended.
pub const COMPANY_SUSPENDED: &str = "Arrêt : l’entreprise est suspendue";

pub struct RunAccess {
    access: Arc<AccessService>,
    tenant_id: Uuid,
    user_id: Uuid,
    fence: LeaseFence,
}

impl RunAccess {
    pub fn new(
        access: Arc<AccessService>,
        tenant_id: Uuid,
        user_id: Uuid,
        fence: LeaseFence,
    ) -> Self {
        Self {
            access,
            tenant_id,
            user_id,
            fence,
        }
    }
}

/// Why a run may not go on for whom it acts for, given their access as it
/// stands (`None`: their account is no longer active); `None` when it may.
/// Read before each step, and before a queued run starts (ADR-085).
pub(super) fn stopped_by(context: Option<&AccessContext>) -> Option<&'static str> {
    match context {
        None => Some(ACCOUNT_INACTIVE),
        Some(context) if context.tenant.status != "active" => Some(COMPANY_SUSPENDED),
        Some(context) if !context.has("agent:use") => Some(AGENT_USE_WITHDRAWN),
        Some(_) => None,
    }
}

#[async_trait]
impl AccessSource for RunAccess {
    async fn current(&self) -> anyhow::Result<StepAccess> {
        self.fence.check()?;
        let context = self
            .access
            .active_context(self.tenant_id, self.user_id)
            .await?;
        Ok(match (stopped_by(context.as_ref()), context) {
            (None, Some(context)) => StepAccess::Granted(CurrentAccess {
                engine: context.engine,
                subject: context.subject,
            }),
            (why, _) => StepAccess::Ended(why.unwrap_or(ACCOUNT_INACTIVE).into()),
        })
    }
}
