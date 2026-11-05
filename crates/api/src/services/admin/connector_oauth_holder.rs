//! Who authorizes an MCP server declared with OAuth, changed in place by a
//! company-wide connector administrator (ADR-174): the company, once for
//! every run, or each person for themself. Nothing is discovered nor
//! registered again: the declaration is kept, its holder changed. The
//! authorizations given under the former holder go, the company's and every
//! person's, their tokens revoked at the authorization server once the
//! change is committed (ADR-172); an authorization started before the change
//! is refused on its return, its declaration having changed.

use mimorya_connectors::mcp::oauth::OAuthHolder;
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{
    ConnectorPatch, get_connector_instance, update_connector_instance,
};
use serde_json::json;
use uuid::Uuid;

use super::connector_authorization::{revoke, take_authorizations};
use super::connector_oauth::McpServer;
use super::connectors::{ConnectorAdmin, PERMISSION};
use super::mcp_config::mcp_config;
use super::{audit_entry, begin_change};
use crate::services::AccessContext;

/// Sets who authorizes the server; returns whether it changed.
pub async fn set_oauth_holder(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
    holder: OAuthHolder,
) -> anyhow::Result<bool> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let server = McpServer::of(get_connector_instance(&mut tx, id).await?)?;
    let Some(previous) = server.declared else {
        let refused = if server.unreadable {
            "La déclaration OAuth de ce serveur est illisible : déclarez-la de nouveau"
        } else {
            "Déclarez d’abord OAuth sur ce serveur"
        };
        return Err(DomainError::validation(refused).into());
    };
    if previous.holder == holder {
        return Ok(false);
    }
    let mut declaration = previous.clone();
    declaration.holder = holder;
    let patch = ConnectorPatch {
        config: Some(mcp_config(&server.prefix, &server.url, Some(&declaration))),
        reachability_changed: true,
        ..ConnectorPatch::default()
    };
    update_connector_instance(&mut tx, id, patch).await?;
    let tenant_id = access.subject.tenant_id;
    let taken = take_authorizations(&mut tx, admin, tenant_id, id, Some(&previous)).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "connector.oauth_holder_changed", "connector", id).metadata(json!({
            "perPerson": holder == OAuthHolder::Person,
            "authorizationRemoved": taken.as_ref().is_some_and(|t| t.company_taken()),
            "personAuthorizationsRemoved": taken.as_ref().map_or(0, |t| t.persons_taken()),
        })),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    if let Some(taken) = taken {
        revoke(admin, access, taken).await;
    }
    Ok(true)
}
