//! Logins and sessions. `find_login` and `resolve_session` are the only reads
//! across tenants: they go through minimal SECURITY DEFINER functions and
//! return just enough to establish the tenant context.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoginRecord {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub password_hash: Option<String>,
    pub status: String,
    pub tenant_status: String,
}

pub async fn find_login(pool: &PgPool, email: &str) -> sqlx::Result<Option<LoginRecord>> {
    sqlx::query_as(
        "select user_id, tenant_id, password_hash, status, tenant_status from auth_find_login($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ResolvedSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub absolute_expires_at: DateTime<Utc>,
}

/// Only valid sessions of active users in active tenants are returned.
pub async fn resolve_session(
    pool: &PgPool,
    token_hash: &str,
) -> sqlx::Result<Option<ResolvedSession>> {
    sqlx::query_as(
        "select session_id, user_id, tenant_id, expires_at, absolute_expires_at \
         from auth_resolve_session($1)",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub absolute_expires_at: DateTime<Utc>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

const MAX_USER_AGENT_CHARS: usize = 500;

/// The account as stored now, its row locked until the end of the
/// transaction: a login opens its session only if the password it verified is
/// still the stored one and the account still active, and a reset, a
/// disabling or a password change waits for it (then ends that session).
pub async fn lock_login_state(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<(Option<String>, String)>> {
    sqlx::query_as("select password_hash, status from users where id = $1 for no key update")
        .bind(user_id)
        .fetch_optional(conn)
        .await
}

/// Opens a session and records the login time.
pub async fn create_session(conn: &mut PgConnection, input: NewSession) -> sqlx::Result<Uuid> {
    let user_agent = input
        .user_agent
        .map(|ua| ua.chars().take(MAX_USER_AGENT_CHARS).collect::<String>());
    let id: Uuid = sqlx::query_scalar(
        "insert into sessions (tenant_id, user_id, token_hash, expires_at, absolute_expires_at, ip, user_agent) \
         values ($1, $2, $3, $4, $5, $6::inet, $7) returning id",
    )
    .bind(input.tenant_id)
    .bind(input.user_id)
    .bind(&input.token_hash)
    .bind(input.expires_at)
    .bind(input.absolute_expires_at)
    .bind(input.ip)
    .bind(user_agent)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("update users set last_login_at = now(), updated_at = now() where id = $1")
        .bind(input.user_id)
        .execute(&mut *conn)
        .await?;
    Ok(id)
}

pub async fn touch_session(
    conn: &mut PgConnection,
    session_id: Uuid,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update sessions set last_seen_at = now(), expires_at = $2 where id = $1 and revoked_at is null",
    )
    .bind(session_id)
    .bind(expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Whether the session is still open; its row stays locked until the end of
/// the transaction, so it cannot be revoked meanwhile.
pub async fn lock_open_session(conn: &mut PgConnection, session_id: Uuid) -> sqlx::Result<bool> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "select id from sessions where id = $1 and revoked_at is null and expires_at > now() \
         and absolute_expires_at > now() for update",
    )
    .bind(session_id)
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

pub async fn revoke_session(conn: &mut PgConnection, session_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("update sessions set revoked_at = now() where id = $1")
        .bind(session_id)
        .execute(conn)
        .await?;
    Ok(())
}
