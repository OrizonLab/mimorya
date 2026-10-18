//! The signed-in user's own account: changing their password (e.g. after an
//! administrator gave them a temporary one). Other sessions end; the caller's
//! stays.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::users::{get_password_hash, revoke_user_sessions, set_password_hash};
use uuid::Uuid;

use crate::auth::credentials::{hash_password_async, verify_password_async};
use crate::services::AccessContext;

pub async fn change_password(
    db: &Database,
    access: &AccessContext,
    session_id: Uuid,
    current_password: String,
    new_password: String,
) -> anyhow::Result<()> {
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    let current_hash = get_password_hash(&mut tx, user_id).await?;
    tx.commit().await?;
    if !verify_password_async(current_hash, current_password).await {
        return Err(DomainError::forbidden("Mot de passe actuel incorrect").into());
    }

    let hash = hash_password_async(new_password).await?;
    let mut tx = db.begin(tenant_id).await?;
    set_password_hash(&mut tx, user_id, &hash).await?;
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
    Ok(())
}
