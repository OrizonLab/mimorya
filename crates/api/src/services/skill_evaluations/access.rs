//! The rights a case of an evaluation reads before each step (ADR-104), as
//! a run does (ADR-035): those of whom asked for the evaluation, as they
//! stand. The case stops once their account is no longer active, they may
//! no longer use the agent, or their company is suspended.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, AccessSource, CurrentAccess, StepAccess};
use uuid::Uuid;

use crate::services::AccessService;
use crate::services::runs::stopped_by;

pub struct EvaluationAccess {
    pub access: Arc<AccessService>,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
}

#[async_trait]
impl AccessSource for EvaluationAccess {
    async fn current(&self) -> anyhow::Result<StepAccess> {
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
