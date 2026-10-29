//! The codes people link their Telegram account with (ADR-133): asked for
//! in their own session, kept as their digest only, valid a while. Telegram
//! sends one back from a person's private chat with the bot: that Telegram
//! user is kept with it, once, until the session that asked for it confirms
//! the link, which takes it. One per person and gateway; the expired ones
//! are forgotten by the maintenance pass.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A code a person asked for, and whom Telegram sent it back from, if it
/// did.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LinkCodeRow {
    pub expires_at: DateTime<Utc>,
    pub claimed_external_id: Option<String>,
    pub claimed_name: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
}

/// A Telegram user a code was sent back from.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Claim {
    pub external_id: String,
    pub name: String,
}

/// Keeps the code `code_hash` the person `user_id` asked for on the
/// gateway, until `expires_at`: their former one, claimed or not, is
/// replaced.
pub async fn put_link_code(
    conn: &mut PgConnection,
    (tenant_id, gateway_id, user_id): (Uuid, Uuid, Uuid),
    code_hash: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into gateway_link_codes (tenant_id, gateway_id, user_id, code_hash, expires_at) \
         values ($1, $2, $3, $4, $5) on conflict (gateway_id, user_id) do update \
         set code_hash = excluded.code_hash, expires_at = excluded.expires_at, \
         claimed_external_id = null, claimed_name = null, claimed_at = null, created_at = now()",
    )
    .bind(tenant_id)
    .bind(gateway_id)
    .bind(user_id)
    .bind(code_hash)
    .bind(expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Keeps that Telegram sent the code `code_hash` back from `claim`, if it
/// is the gateway's, unexpired at `now` and not sent back already; whose
/// code it was.
pub async fn claim_link_code(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    code_hash: &str,
    now: DateTime<Utc>,
    claim: &Claim,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "update gateway_link_codes set claimed_external_id = $4, claimed_name = $5, \
         claimed_at = $3 where gateway_id = $1 and code_hash = $2 and expires_at > $3 \
         and claimed_at is null returning user_id",
    )
    .bind(gateway_id)
    .bind(code_hash)
    .bind(now)
    .bind(&claim.external_id)
    .bind(&claim.name)
    .fetch_optional(conn)
    .await
}

/// The person's code on the gateway, if unexpired at `now`.
pub async fn link_code_of(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<Option<LinkCodeRow>> {
    sqlx::query_as(
        "select expires_at, claimed_external_id, claimed_name, claimed_at \
         from gateway_link_codes where gateway_id = $1 and user_id = $2 and expires_at > $3",
    )
    .bind(gateway_id)
    .bind(user_id)
    .bind(now)
    .fetch_optional(conn)
    .await
}

/// Takes the person's code on the gateway once Telegram sent it back,
/// unexpired at `now`: whom from, to link.
pub async fn take_claim(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<Option<Claim>> {
    sqlx::query_as(
        "delete from gateway_link_codes where gateway_id = $1 and user_id = $2 \
         and expires_at > $3 and claimed_at is not null \
         returning claimed_external_id as external_id, claimed_name as name",
    )
    .bind(gateway_id)
    .bind(user_id)
    .bind(now)
    .fetch_optional(conn)
    .await
}

/// Forgets the person's code on the gateway; whether there was one.
pub async fn delete_link_code(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let deleted =
        sqlx::query("delete from gateway_link_codes where gateway_id = $1 and user_id = $2")
            .bind(gateway_id)
            .bind(user_id)
            .execute(conn)
            .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Forgets every company's codes expired before `before`, through a
/// narrowly scoped database function; how many.
pub async fn purge_link_codes(pool: &PgPool, before: DateTime<Utc>) -> sqlx::Result<i64> {
    sqlx::query_scalar("select system_purge_gateway_link_codes($1)")
        .bind(before)
        .fetch_one(pool)
        .await
}
