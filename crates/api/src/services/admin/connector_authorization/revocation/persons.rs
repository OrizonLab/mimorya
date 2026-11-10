//! A person's own authorizations taken, with what revoking them needs
//! (ADR-174): the one they withdraw, and all of theirs when their account
//! is disabled, one revocation per connector under the declaration it
//! holds then; nothing is sent under another declaration (`sending`).

use std::collections::BTreeMap;

use mimorya_connectors::mcp::oauth::OAuthDeclaration;
use mimorya_db::TenantTx;
use mimorya_db::repositories::connector_person_authorizations::{
    TakenPersonAuthorization, take_person_authorization, take_person_authorizations_of_user,
};
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::super::super::connector_oauth::McpServer;
use super::super::super::connectors::ConnectorAdmin;
use super::{Revocation, taken};

/// The declaration the connector holds now, if it can be read.
pub(super) async fn declared(
    tx: &mut TenantTx,
    connector_id: Uuid,
) -> anyhow::Result<Option<OAuthDeclaration>> {
    let instance = get_connector_instance(tx, connector_id).await?;
    Ok(McpServer::of(instance)
        .ok()
        .and_then(|server| server.declared))
}

/// Takes the person's own authorization of the connector, if they gave one.
pub async fn take_person(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    connector_id: Uuid,
    user_id: Uuid,
) -> anyhow::Result<Option<Revocation>> {
    let Some(person) = take_person_authorization(tx, connector_id, user_id).await? else {
        return Ok(None);
    };
    let declared = declared(tx, connector_id).await?;
    taken(
        tx,
        admin,
        tenant_id,
        connector_id,
        declared.as_ref(),
        None,
        vec![person],
    )
    .await
}

/// Takes every authorization the person gave, their account disabled: one
/// revocation per connector.
pub async fn take_persons_of_user(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> anyhow::Result<Vec<Revocation>> {
    let mut by_connector: BTreeMap<Uuid, Vec<TakenPersonAuthorization>> = BTreeMap::new();
    for person in take_person_authorizations_of_user(tx, user_id).await? {
        by_connector
            .entry(person.connector_id)
            .or_default()
            .push(person);
    }
    let mut revocations = Vec::with_capacity(by_connector.len());
    for (connector_id, persons) in by_connector {
        let declared = declared(tx, connector_id).await?;
        let declared = declared.as_ref();
        if let Some(revocation) =
            taken(tx, admin, tenant_id, connector_id, declared, None, persons).await?
        {
            revocations.push(revocation);
        }
    }
    Ok(revocations)
}
