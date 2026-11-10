//! What a disabled account loses at once, whoever disables it: an
//! administrator (ADR-035) or the company's identity provider through SCIM
//! (ADR-183). In the change's transaction: its sessions, its pending
//! invitation, the accounts it linked at its company's providers (ADR-116),
//! its identities on the gateways (ADR-125), and its own authorizations of
//! MCP servers (ADR-174), taken; once committed, those revoked at their
//! authorization servers, its linked accounts' grants at a provider that
//! revokes one (ADR-194), and its kept connector sessions forgotten.

use mimorya_db::TenantTx;
use mimorya_db::repositories::account_links::take_account_links_of;
use mimorya_db::repositories::gateway_identities::delete_gateway_identities_of;
use mimorya_db::repositories::invitations::delete_invitation_of;
use mimorya_db::repositories::users::revoke_user_sessions;
use uuid::Uuid;

use super::connector_authorization::{Revocation, revoke_all_by, take_persons_of_user};
use super::connectors::ConnectorAdmin;
use crate::services::account_links::LinksLetGo;

/// What a disabled account held, taken in the transaction that disables
/// it, to be revoked once committed.
#[derive(Default)]
pub(crate) struct TakenAccess {
    /// Its authorizations of MCP servers (ADR-174).
    revocations: Vec<Revocation>,
    /// Its linked accounts at a provider that revokes a grant (ADR-194).
    links: Vec<LinksLetGo>,
}

/// Takes from the account `user_id`, in the transaction `tx` that disables
/// it, what it may no longer use; what is to be revoked once committed.
pub(crate) async fn take_access(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> anyhow::Result<TakenAccess> {
    revoke_user_sessions(tx, user_id, None).await?;
    delete_invitation_of(tx, user_id).await?;
    // What they granted at a provider is no longer anyone's to use
    // (ADR-116): taken now, revoked once committed where it can be
    // (ADR-194).
    let links = take_account_links_of(tx, user_id).await?;
    let links = admin.account_links.let_go_of(tx, links).await?;
    // Nor may their identity on a gateway speak for them (ADR-125).
    delete_gateway_identities_of(tx, user_id).await?;
    // Nor may what they authorized MCP servers to do for them be used
    // (ADR-174): taken now, revoked once committed.
    let revocations = take_persons_of_user(tx, admin, tenant_id, user_id).await?;
    Ok(TakenAccess { revocations, links })
}

/// Once the disabling of `user_id` is committed: its kept connector
/// sessions forgotten, what was `taken` revoked, audited as let go by `by`
/// (an administrator), or by the platform, the linked accounts' grants and
/// the MCP servers' authorizations at once.
pub(crate) async fn let_go(
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    by: Option<Uuid>,
    user_id: Uuid,
    taken: TakenAccess,
) {
    let TakenAccess { revocations, links } = taken;
    let connectors: Vec<Uuid> = revocations.iter().map(|r| r.connector_id).collect();
    // The revocations' tasks started at once, before the kept sessions are
    // forgotten: a request abandoned meanwhile leaves them going.
    let servers = async {
        if !revocations.is_empty() {
            revoke_all_by(admin, tenant_id, by, revocations).await;
        }
    };
    let accounts = admin.account_links.revoke(tenant_id, by, links);
    let forgotten = async {
        for connector_id in connectors {
            admin.connectors.forget_person(connector_id, user_id).await;
        }
    };
    futures::join!(servers, accounts, forgotten);
}
