//! Password reset links (ADR-028). `find_password_reset` is the only read
//! across tenants (a SECURITY DEFINER function, before sign-in), with the
//! periodic purge of expired links; the rest runs in the account's tenant.
//! A change of password or status deletes the account's link (trigger of
//! migration 0011).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PasswordResetRecord {
    pub reset_id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub expires_at: DateTime<Utc>,
    pub user_status: String,
    pub tenant_status: String,
}

pub async fn find_password_reset(
    pool: &PgPool,
    token_hash: &str,
) -> sqlx::Result<Option<PasswordResetRecord>> {
    sqlx::query_as(
        "select reset_id, tenant_id, user_id, email, expires_at, user_status, tenant_status \
         from auth_find_password_reset($1)",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

/// Who a link is sent to, the account's row locked until the end of the
/// transaction (as a login, a reset or a disabling locks it).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ResetRecipient {
    pub email: String,
    pub display_name: String,
    pub company: String,
    pub status: String,
    pub has_password: bool,
}

pub async fn lock_reset_recipient(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<ResetRecipient>> {
    sqlx::query_as(
        "select u.email::text as email, u.display_name, t.name as company, u.status, \
         u.password_hash is not null as has_password \
         from users u join tenants t on t.id = u.tenant_id where u.id = $1 for no key update of u",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

pub struct NewPasswordReset<'a> {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub token_hash: &'a str,
    pub expires_at: DateTime<Utc>,
}

/// Replaces the account's pending link, if any: a former link stops working.
pub async fn put_password_reset(
    conn: &mut PgConnection,
    input: NewPasswordReset<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into password_resets (tenant_id, user_id, token_hash, expires_at) \
         values ($1, $2, $3, $4) on conflict (user_id) do update set \
         token_hash = excluded.token_hash, created_at = now(), expires_at = excluded.expires_at",
    )
    .bind(input.tenant_id)
    .bind(input.user_id)
    .bind(input.token_hash)
    .bind(input.expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Takes the link for its single use: deleted, and its account returned,
/// only if it still holds this token and has not expired when this statement
/// runs (after any wait for the account's row).
pub async fn take_password_reset(
    conn: &mut PgConnection,
    reset_id: Uuid,
    token_hash: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "delete from password_resets where id = $1 and token_hash = $2 \
         and expires_at > statement_timestamp() returning user_id",
    )
    .bind(reset_id)
    .bind(token_hash)
    .fetch_optional(conn)
    .await
}

/// Deletes the expired links of every company; returns how many.
pub async fn purge_expired_password_resets(pool: &PgPool) -> sqlx::Result<i32> {
    sqlx::query_scalar("select system_purge_expired_password_resets()")
        .fetch_one(pool)
        .await
}
