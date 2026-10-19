//! The rights a run reads before each step (ADR-035): its user's access as it
//! stands now, through the same service as requests. The company's rules come
//! from its cache, invalidated by every administration change; the account,
//! its assignments and attributes from the database.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_agent::runtime::{AccessSource, CurrentAccess};
use mimorya_core::{DomainError, ErrorCode};
use uuid::Uuid;

use crate::services::access::AccessService;

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
    async fn current(&self) -> anyhow::Result<Option<CurrentAccess>> {
        match self.access.user_context(self.tenant_id, self.user_id).await {
            Ok(context) => Ok(Some(CurrentAccess {
                engine: context.engine,
                subject: context.subject,
            })),
            // The account is no longer active (disabled, or gone).
            Err(error)
                if error
                    .downcast_ref::<DomainError>()
                    .is_some_and(|e| e.code == ErrorCode::Forbidden) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}
