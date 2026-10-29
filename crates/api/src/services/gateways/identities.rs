//! The gateways a person may talk with the agent through, and who they are
//! on each (ADR-125): listed for them alone, removed by them; their
//! company's administrators see how many people linked theirs, never who.

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::{
    GatewayIdentityRow, delete_gateway_identity, list_gateway_identities_of,
};
use mimorya_db::repositories::gateways::{GatewayRow, list_gateways};
use serde_json::json;
use uuid::Uuid;

use super::GatewayService;
use super::telegram::PendingLink;
use crate::services::AccessContext;

/// What linking requires: talking with the agent.
const AGENT_USE: &str = "agent:use";

/// A gateway of the person's company, and their identity there.
#[derive(Debug, Clone)]
pub struct GatewayOfPerson {
    pub gateway: GatewayRow,
    /// A link may be made: the gateway is enabled and the person may talk
    /// with the agent. Otherwise their identity stays theirs to remove.
    pub available: bool,
    pub identity: Option<GatewayIdentityRow>,
    /// A Telegram link under way (ADR-133).
    pub pending: Option<PendingLink>,
}

impl GatewayService {
    /// The company's gateways, and the person's identity on each.
    pub async fn gateways_of(
        &self,
        access: &AccessContext,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Vec<GatewayOfPerson>> {
        let may_use = access.has(AGENT_USE);
        let user_id = access.subject.user_id;
        let mut tx = self.db.begin(access.subject.tenant_id).await?;
        let gateways = list_gateways(&mut tx).await?;
        let mut identities = list_gateway_identities_of(&mut tx, user_id).await?;
        let mut listed = Vec::with_capacity(gateways.len());
        for gateway in gateways {
            let identity = identities
                .iter()
                .position(|identity| identity.gateway_id == gateway.id)
                .map(|at| identities.swap_remove(at));
            let pending = self.pending_link(&mut tx, &gateway, user_id, now).await?;
            listed.push(GatewayOfPerson {
                available: gateway.enabled && may_use,
                gateway,
                identity,
                pending,
            });
        }
        tx.commit().await?;
        Ok(listed)
    }

    /// Removes the person's identity on the gateway `gateway_id`.
    pub async fn unlink(&self, access: &AccessContext, gateway_id: Uuid) -> anyhow::Result<()> {
        let mut tx = self.db.begin(access.subject.tenant_id).await?;
        let Some(external_id) =
            delete_gateway_identity(&mut tx, gateway_id, access.subject.user_id).await?
        else {
            return Err(DomainError::not_found("Aucun compte relié à cette passerelle").into());
        };
        let entry = AuditEntry::new(
            access.subject.tenant_id,
            "user",
            "gateway.identity_removed",
            "success",
        )
        .by_user(access.subject.user_id)
        .resource("gateway", gateway_id)
        .metadata(json!({ "externalId": external_id }));
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(())
    }
}
