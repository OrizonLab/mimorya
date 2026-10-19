//! Invitation links issued by the operator (ADR-027, ADR-033): for a new
//! company's first administrator, or again for an account that has not yet
//! chosen its password (its link expired or was lost, and no administrator
//! of its company can issue another one).

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::auth::find_login;
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::system_entry;
use crate::services::invitations::{RenewedInvitation, invitation_link, renew_invitation};

/// A link for the operator to transmit; it replaced any former one.
pub struct OperatorInvitation {
    pub link: String,
    pub expires_at: DateTime<Utc>,
}

/// Stores a new invitation of `user_id`, with its audit entry, in the
/// caller's transaction.
pub(super) async fn issue(
    tx: &mut TenantTx,
    user_id: Uuid,
    web_origin: &str,
) -> anyhow::Result<OperatorInvitation> {
    let RenewedInvitation { token, expires_at } = renew_invitation(tx, user_id, None).await?;
    let tenant_id = tx.tenant_id();
    append_audit(
        tx,
        system_entry(tenant_id, "user.invited", "user", user_id)
            .metadata(json!({ "expiresAt": expires_at, "delivery": "operator" })),
    )
    .await?;
    Ok(OperatorInvitation {
        link: invitation_link(web_origin, &token),
        expires_at,
    })
}

/// A new link for the account at `email`, if it is still waiting for its
/// password and its company is active; the former link stops working.
pub async fn reinvite(
    db: &Database,
    web_origin: &str,
    email: &str,
) -> anyhow::Result<OperatorInvitation> {
    let email = email.trim().to_lowercase();
    let Some(login) = find_login(db.pool(), &email).await? else {
        return Err(DomainError::not_found("Aucun compte à cette adresse").into());
    };
    if login.tenant_status != "active" {
        return Err(DomainError::conflict(
            "L’entreprise de ce compte est suspendue : pas d’invitation",
        )
        .into());
    }
    let mut tx = db.begin(login.tenant_id).await?;
    let invitation = issue(&mut tx, login.user_id, web_origin).await?;
    tx.commit().await?;
    Ok(invitation)
}
