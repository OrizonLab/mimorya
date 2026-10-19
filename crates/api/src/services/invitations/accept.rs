//! What an invitation link opens before sign-in: who is invited and by
//! which company, then the password the person chooses, which activates the
//! account and opens a session. A link works once, until it expires, for an
//! account still invited in an active company; every other link gets the
//! same answer, so nothing tells them apart.

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::invitations::{InvitationRecord, find_invitation, take_invitation};
use mimorya_db::repositories::users::activate_invited_user;

use crate::auth::credentials::{hash_password_async, hash_token};
use crate::auth::sessions::open_session;
use crate::plugins::RequestContext;

const UNUSABLE: &str =
    "Ce lien d’invitation n’est plus valable : demandez-en un nouveau à votre administrateur";

pub struct InvitationView {
    pub email: String,
    pub display_name: String,
    pub company: String,
    pub expires_at: DateTime<Utc>,
}

async fn usable(db: &Database, token: &str) -> anyhow::Result<InvitationRecord> {
    match find_invitation(db.pool(), &hash_token(token)).await? {
        Some(record)
            if record.expires_at > Utc::now()
                && record.user_status == "invited"
                && record.tenant_status == "active" =>
        {
            Ok(record)
        }
        _ => Err(DomainError::not_found(UNUSABLE).into()),
    }
}

pub async fn inspect_invitation(db: &Database, token: &str) -> anyhow::Result<InvitationView> {
    let record = usable(db, token).await?;
    Ok(InvitationView {
        email: record.email,
        display_name: record.display_name,
        company: record.company,
        expires_at: record.expires_at,
    })
}

/// Activates the account with its chosen password; returns the token of
/// the session it opens.
pub async fn accept_invitation(
    db: &Database,
    token: &str,
    password: String,
    context: &RequestContext,
    headers: &HeaderMap,
) -> anyhow::Result<String> {
    let record = usable(db, token).await?;
    let password_hash = hash_password_async(password).await?;
    let mut tx = db.begin(record.tenant_id).await?;
    // Deleted on use: a concurrent acceptance finds nothing left.
    let taken = take_invitation(&mut tx, record.invitation_id).await?;
    let still_valid = taken
        .is_some_and(|(user_id, expires_at)| user_id == record.user_id && expires_at > Utc::now());
    if !still_valid || !activate_invited_user(&mut tx, record.user_id, &password_hash).await? {
        return Err(DomainError::not_found(UNUSABLE).into());
    }
    let session = open_session(&mut tx, record.user_id, context, headers).await?;
    let mut entry = AuditEntry::new(
        record.tenant_id,
        "user",
        "user.invitation_accepted",
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
