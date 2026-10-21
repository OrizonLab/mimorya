//! What a reset link opens before sign-in (the account's address), then the
//! new password, which ends every session of the account and opens one. A
//! link works once, until it expires, for an account still active in an
//! active company; every other link gets the same answer.

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::auth::lock_login_state;
use mimorya_db::repositories::password_resets::{
    PasswordResetRecord, find_password_reset, take_password_reset,
};
use mimorya_db::repositories::users::{revoke_user_sessions, set_reset_password_hash};

use crate::auth::credentials::{hash_password_async, hash_token};
use crate::auth::sessions::open_session;
use crate::plugins::RequestContext;
use crate::services::AccessService;
use crate::services::sso::required_provider;

const UNUSABLE: &str = "Ce lien de réinitialisation n’est plus valable : demandez-en un nouveau \
     depuis la page de connexion";

pub struct ResetView {
    pub email: String,
    pub expires_at: DateTime<Utc>,
}

async fn usable(db: &Database, token_hash: &str) -> anyhow::Result<PasswordResetRecord> {
    match find_password_reset(db.pool(), token_hash).await? {
        Some(record)
            if record.expires_at > Utc::now()
                && record.user_status == "active"
                && record.tenant_status == "active" =>
        {
            Ok(record)
        }
        _ => Err(DomainError::not_found(UNUSABLE).into()),
    }
}

pub async fn inspect_reset(db: &Database, token: &str) -> anyhow::Result<ResetView> {
    let record = usable(db, &hash_token(token)).await?;
    Ok(ResetView {
        email: record.email,
        expires_at: record.expires_at,
    })
}

/// Sets the chosen password; returns the token of the session it opens.
/// Under a provider required for its domain, a link sent before is of no
/// use (ADR-066).
pub async fn complete_reset(
    db: &Database,
    access: &AccessService,
    token: &str,
    password: String,
    context: &RequestContext,
    headers: &HeaderMap,
) -> anyhow::Result<String> {
    let token_hash = hash_token(token);
    let record = usable(db, &token_hash).await?;
    if let Some(provider) = required_provider(db, access, record.tenant_id, record.user_id).await? {
        return Err(DomainError::forbidden(format!(
            "Votre entreprise impose la connexion par {provider} : ce lien ne sert plus."
        ))
        .into());
    }
    let password_hash = hash_password_async(password).await?;
    let mut tx = db.begin(record.tenant_id).await?;
    // The account's row first, as a login, an administrator's reset or a
    // disabling: they wait for each other instead of crossing.
    let active = lock_login_state(&mut tx, record.user_id)
        .await?
        .is_some_and(|(_, status)| status == "active");
    // Deleted on use, and only with this very token: a concurrent use, a
    // newer link or a password changed meanwhile leaves nothing to take.
    let taken = take_password_reset(&mut tx, record.reset_id, &token_hash).await?;
    if !active || taken != Some(record.user_id) {
        return Err(DomainError::not_found(UNUSABLE).into());
    }
    set_reset_password_hash(&mut tx, record.user_id, &password_hash).await?;
    // Every session ends, a stolen one included; only the new one is open.
    revoke_user_sessions(&mut tx, record.user_id, None).await?;
    let session = open_session(&mut tx, record.user_id, context, headers).await?;
    let mut entry = AuditEntry::new(
        record.tenant_id,
        "user",
        "user.password_reset_completed",
        "success",
    )
    .by_user(record.user_id)
    .resource("user", record.user_id)
    .ip(context.ip_string());
    entry.request_id = Some(context.id.to_string());
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(session)
}
