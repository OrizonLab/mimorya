//! Invitation links of new accounts (ADR-027). `find_invitation` is the only
//! read across tenants (a SECURITY DEFINER function, before sign-in); the
//! rest runs in the invited account's tenant.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InvitationRecord {
    pub invitation_id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub company: String,
    pub expires_at: DateTime<Utc>,
    pub user_status: String,
    pub tenant_status: String,
}

pub async fn find_invitation(
    pool: &PgPool,
    token_hash: &str,
) -> sqlx::Result<Option<InvitationRecord>> {
    sqlx::query_as(
        "select invitation_id, tenant_id, user_id, email, display_name, company, expires_at, \
         user_status, tenant_status from auth_find_invitation($1)",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub struct NewInvitation<'a> {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub token_hash: &'a str,
    /// `None` when the platform's operator issued it (ADR-033).
    pub created_by_user_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

/// Replaces the account's pending invitation, if any: a former link stops
/// working.
pub async fn put_invitation(conn: &mut PgConnection, input: NewInvitation<'_>) -> sqlx::Result<()> {
    sqlx::query(
        "insert into user_invitations (tenant_id, user_id, token_hash, created_by_user_id, \
         expires_at) values ($1, $2, $3, $4, $5) on conflict (user_id) do update set \
         token_hash = excluded.token_hash, created_by_user_id = excluded.created_by_user_id, \
         created_at = now(), expires_at = excluded.expires_at",
    )
    .bind(input.tenant_id)
    .bind(input.user_id)
    .bind(input.token_hash)
    .bind(input.created_by_user_id)
    .bind(input.expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Takes the invitation for its single use: deleted, and its account
/// returned, only if it still holds this token and has not expired when this
/// statement runs, after any wait for the account's row (a concurrent
/// acceptance, or a resent link, leaves `None`).
pub async fn take_invitation(
    conn: &mut PgConnection,
    invitation_id: Uuid,
    token_hash: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "delete from user_invitations where id = $1 and token_hash = $2 \
         and expires_at > statement_timestamp() returning user_id",
    )
    .bind(invitation_id)
    .bind(token_hash)
    .fetch_optional(conn)
    .await
}

/// When the pending invitation of each account expires.
pub async fn pending_invitations(
    conn: &mut PgConnection,
) -> sqlx::Result<HashMap<Uuid, DateTime<Utc>>> {
    let rows: Vec<(Uuid, DateTime<Utc>)> =
        sqlx::query_as("select user_id, expires_at from user_invitations")
            .fetch_all(conn)
            .await?;
    Ok(rows.into_iter().collect())
}

pub async fn delete_invitation_of(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("delete from user_invitations where user_id = $1")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(())
}
