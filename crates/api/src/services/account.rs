//! The signed-in user's own account: changing their password (e.g. after an
//! administrator gave them a temporary one). Other sessions end; the caller's
//! stays. A reset or a revocation made meanwhile by an administrator wins:
//! the change is refused rather than silently undoing it.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::auth::lock_open_session;
use mimorya_db::repositories::users::{
    get_password_hash, replace_password_hash, revoke_user_sessions,
};
use uuid::Uuid;

use crate::auth::credentials::{hash_password_async, verify_password_async};
use crate::services::AccessContext;

/// How a change ended, when no race or failure refused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordChange {
    Changed,
    /// The only outcome that counts as a guess of the current password.
    WrongCurrentPassword,
}

pub async fn change_password(
    db: &Database,
    access: &AccessContext,
    session_id: Uuid,
    current_password: String,
    new_password: String,
) -> anyhow::Result<PasswordChange> {
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    let current_hash = get_password_hash(&mut tx, user_id).await?;
    tx.commit().await?;
    let verified = verify_password_async(current_hash.clone(), current_password).await;
    let (true, Some(current_hash)) = (verified, current_hash) else {
        return Ok(PasswordChange::WrongCurrentPassword);
    };

    let new_hash = hash_password_async(new_password).await?;
    let mut tx = db.begin(tenant_id).await?;
    // The account row first, then the sessions: the order a reset uses too.
    if !replace_password_hash(&mut tx, user_id, &current_hash, &new_hash).await? {
        return Err(DomainError::conflict(
            "Votre mot de passe vient d’être modifié par ailleurs : reconnectez-vous.",
        )
        .into());
    }
    if !lock_open_session(&mut tx, session_id).await? {
        return Err(DomainError::unauthenticated("Session expirée : reconnectez-vous.").into());
    }
    // Other sessions (another device, a stolen cookie) end; this one stays.
    revoke_user_sessions(&mut tx, user_id, Some(session_id)).await?;
    append_audit(
        &mut tx,
        AuditEntry::new(tenant_id, "user", "user.password_changed", "success")
            .by_user(user_id)
            .resource("user", user_id),
    )
    .await?;
    tx.commit().await?;
    Ok(PasswordChange::Changed)
}
