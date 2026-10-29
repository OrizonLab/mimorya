//! The codes people link their Telegram account with (ADR-133): asked for
//! in their own session, kept as their digest only, valid a while. Telegram
//! sends one back from a person's private chat with the bot: it is offered
//! to that Telegram user, who is told whose account it links; once they
//! accept, they are kept with it, once, until the session that asked for it
//! confirms the link, which takes it. One per person and gateway; the
//! expired ones are forgotten by the maintenance pass.

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
         offered_external_id = null, offered_at = null, claimed_external_id = null, \
         claimed_name = null, claimed_at = null, created_at = now()",
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

/// Whose the code `code_hash` is, if it is the gateway's, unexpired at
/// `now` and not accepted by anyone yet.
pub async fn link_code_owner(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    code_hash: &str,
    now: DateTime<Utc>,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "select user_id from gateway_link_codes where gateway_id = $1 and code_hash = $2 \
         and expires_at > $3 and claimed_at is null",
    )
    .bind(gateway_id)
    .bind(code_hash)
    .bind(now)
    .fetch_optional(conn)
    .await
}

/// Offers the code `code_hash` to the Telegram user `external_id`, who sent
/// it back, if it is the gateway's, unexpired at `now` and not accepted by
/// anyone yet: whose code it is. An earlier offer of it is replaced, and so
/// is any other offer awaiting them: they accept only the last link the bot
/// named to them.
pub async fn offer_link_code(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    code_hash: &str,
    now: DateTime<Utc>,
    external_id: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query(
        "update gateway_link_codes set offered_external_id = null, offered_at = null \
         where gateway_id = $1 and offered_external_id = $2 and claimed_at is null",
    )
    .bind(gateway_id)
    .bind(external_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query_scalar(
        "update gateway_link_codes set offered_external_id = $4, offered_at = $3 \
         where gateway_id = $1 and code_hash = $2 and expires_at > $3 \
         and claimed_at is null returning user_id",
    )
    .bind(gateway_id)
    .bind(code_hash)
    .bind(now)
    .bind(external_id)
    .fetch_optional(conn)
    .await
}

/// The code offered to the Telegram user `claim.external_id` on the gateway
/// (the last link the bot named to them), unexpired at `now` and not
/// accepted yet: accepted by them, kept with how Telegram showed them; whose
/// code it was. Never one that would link them to someone while they are
/// linked to someone else.
pub async fn accept_offered_code(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    now: DateTime<Utc>,
    claim: &Claim,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "update gateway_link_codes set claimed_external_id = offered_external_id, \
         claimed_name = $4, claimed_at = $3 where id = ( \
         select c.id from gateway_link_codes c where c.gateway_id = $1 \
         and c.offered_external_id = $2 and c.expires_at > $3 and c.claimed_at is null \
         and not exists (select 1 from gateway_identities i where i.gateway_id = c.gateway_id \
         and i.external_id = c.offered_external_id and i.user_id <> c.user_id) \
         for update) \
         and claimed_at is null returning user_id",
    )
    .bind(gateway_id)
    .bind(&claim.external_id)
    .bind(now)
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
