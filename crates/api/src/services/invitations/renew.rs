//! A new invitation link for an account that never chose its password,
//! replacing any former one: issued by an administrator of its company
//! (ADR-027) or by the platform's operator (ADR-033).

use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::TenantTx;
use mimorya_db::repositories::auth::lock_login_state;
use mimorya_db::repositories::invitations::{NewInvitation, put_invitation};
use uuid::Uuid;

use super::INVITATION_VALIDITY;
use crate::auth::credentials::new_session_token;

/// The link's token, for the caller to send or show once.
pub struct RenewedInvitation {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// Stores the new link in the caller's transaction. The account's row is
/// locked first, as an acceptance locks it: the status read here holds until
/// the link is stored. `created_by` is `None` for the operator.
pub async fn renew_invitation(
    tx: &mut TenantTx,
    user_id: Uuid,
    created_by: Option<Uuid>,
) -> anyhow::Result<RenewedInvitation> {
    let Some((_, status)) = lock_login_state(tx, user_id).await? else {
        return Err(DomainError::not_found("Compte introuvable").into());
    };
    if status != "invited" {
        return Err(DomainError::conflict(
            "Ce compte a déjà choisi son mot de passe ou est désactivé : pas d’invitation",
        )
        .into());
    }
    let (token, token_hash) = new_session_token();
    let expires_at = Utc::now() + INVITATION_VALIDITY;
    let tenant_id = tx.tenant_id();
    put_invitation(
        tx,
        NewInvitation {
            tenant_id,
            user_id,
            token_hash: &token_hash,
            created_by_user_id: created_by,
            expires_at,
        },
    )
    .await?;
    Ok(RenewedInvitation { token, expires_at })
}
