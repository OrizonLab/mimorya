//! A person's own authorizations of the MCP servers each person authorizes
//! (ADR-174), as their account shows them: the servers they may authorize
//! (they may use connectors, the server is declared for each person), and
//! any server they already authorized, so that they can withdraw it even
//! once they no longer may use it. Never anyone else's. Withdrawn, their
//! tokens go, then are revoked at the authorization server (ADR-172).

use mimorya_connectors::mcp::oauth::OAuthHolder;
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connector_person_authorizations::{
    PersonAuthorizationRow, person_authorizations_of,
};
use mimorya_db::repositories::connectors::list_connector_instances;
use uuid::Uuid;

use super::revocation::{revoke, take_person};
use crate::services::AccessContext;
use crate::services::admin::audit_entry;
use crate::services::admin::connector_oauth::McpServer;
use crate::services::admin::connectors::ConnectorAdmin;
use crate::services::connectors::CONNECTOR_USE;

/// What the audit calls a person withdrawing their own authorization.
const PERSON_REMOVED: &str = "connector.person_oauth_removed";

/// A server the person may authorize, or did.
pub struct PersonServer {
    pub connector_id: Uuid,
    pub name: String,
    /// Whether they may authorize it now: enabled, declared for each
    /// person, and they may use connectors.
    pub available: bool,
    pub authorization: Option<PersonAuthorizationRow>,
}

/// The servers each person authorizes that the signed-in person may
/// authorize, and those they did, by name.
pub async fn list_person_servers(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
) -> anyhow::Result<Vec<PersonServer>> {
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let may_use = access.has(CONNECTOR_USE);
    let mut tx = admin.db.begin(tenant_id).await?;
    let instances = list_connector_instances(&mut tx, false).await?;
    let mut authorizations = person_authorizations_of(&mut tx, user_id).await?;
    tx.commit().await?;
    let mut servers = Vec::new();
    for instance in instances {
        let (id, name, enabled) = (instance.id, instance.name.clone(), instance.enabled);
        let each_person = McpServer::of(Some(instance))
            .ok()
            .and_then(|server| server.declared)
            .is_some_and(|declared| declared.holder == OAuthHolder::Person);
        let position = authorizations.iter().position(|row| row.connector_id == id);
        let authorization = position.map(|at| authorizations.swap_remove(at));
        let available = may_use && enabled && each_person;
        if available || authorization.is_some() {
            servers.push(PersonServer {
                connector_id: id,
                name,
                available,
                authorization,
            });
        }
    }
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers)
}

/// Withdraws the signed-in person's own authorization of the server: a
/// server they did not authorize, or another company's, is answered as
/// absent.
pub async fn remove_person_authorization(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = admin.db.begin(tenant_id).await?;
    let Some(taken) = take_person(&mut tx, admin, tenant_id, id, user_id).await? else {
        return Err(DomainError::not_found("Aucune autorisation de ce serveur").into());
    };
    append_audit(
        &mut tx,
        audit_entry(access, PERSON_REMOVED, "connector", id),
    )
    .await?;
    tx.commit().await?;
    // Their session goes with it here; elsewhere it is never given to a
    // run again, and goes once idle.
    admin.connectors.forget_person(id, user_id).await;
    revoke(admin, access, taken).await;
    Ok(())
}
