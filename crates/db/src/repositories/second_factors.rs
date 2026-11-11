//! An account's second factor (ADR-200): its sealed authenticator secret,
//! the last step a code was accepted for, the codes refused in a row, and
//! its recovery codes, kept as digests. Read and written in the account's
//! company's transaction; the account's deletion takes them with it.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::secrets::StoredSecret;

/// What is kept of an account's second factor.
#[derive(Debug, Clone)]
pub struct SecondFactorRow {
    pub secret: StoredSecret,
    /// `None` while the enrolment waits for its first code.
    pub enabled_at: Option<DateTime<Utc>>,
    pub last_step: i64,
    pub failed_attempts: i32,
}

#[derive(sqlx::FromRow)]
struct Row {
    secret_ciphertext: Vec<u8>,
    secret_iv: Vec<u8>,
    secret_tag: Vec<u8>,
    key_version: i32,
    enabled_at: Option<DateTime<Utc>>,
    last_step: i64,
    failed_attempts: i32,
}

impl From<Row> for SecondFactorRow {
    fn from(row: Row) -> Self {
        Self {
            secret: StoredSecret {
                ciphertext: row.secret_ciphertext,
                iv: row.secret_iv,
                auth_tag: row.secret_tag,
                key_version: row.key_version,
            },
            enabled_at: row.enabled_at,
            last_step: row.last_step,
            failed_attempts: row.failed_attempts,
        }
    }
}

/// The second factor of `user_id`, locked until the end of the
/// transaction: a code checked against it is accepted by one request only.
pub async fn lock_second_factor(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<SecondFactorRow>> {
    let row: Option<Row> = sqlx::query_as(
        "select secret_ciphertext, secret_iv, secret_tag, key_version, enabled_at, last_step, \
         failed_attempts from second_factors where user_id = $1 for update",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(Into::into))
}

/// Whether the account's second factor is on, and since when, with its
/// recovery codes left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct SecondFactorState {
    pub enabled_at: Option<DateTime<Utc>>,
    pub recovery_codes_left: i64,
}

/// `None` when the account has neither a second factor nor an enrolment.
pub async fn second_factor_state(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<SecondFactorState>> {
    sqlx::query_as(
        "select f.enabled_at, (select count(*) from second_factor_recovery_codes c \
           where c.tenant_id = f.tenant_id and c.user_id = f.user_id and c.used_at is null) \
           as recovery_codes_left \
         from second_factors f where f.user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// Starts an enrolment with `secret`, replacing one that waits; never a
/// second factor already on. Whether it was written.
pub async fn put_pending_second_factor(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    secret: &StoredSecret,
) -> sqlx::Result<bool> {
    let written = sqlx::query(
        "insert into second_factors (tenant_id, user_id, secret_ciphertext, secret_iv, \
         secret_tag, key_version) values ($1, $2, $3, $4, $5, $6) \
         on conflict (tenant_id, user_id) do update set \
         secret_ciphertext = excluded.secret_ciphertext, secret_iv = excluded.secret_iv, \
         secret_tag = excluded.secret_tag, key_version = excluded.key_version, \
         last_step = 0, failed_attempts = 0, created_at = now() \
         where second_factors.enabled_at is null",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(&secret.ciphertext)
    .bind(&secret.iv)
    .bind(&secret.auth_tag)
    .bind(secret.key_version)
    .execute(conn)
    .await?;
    Ok(written.rows_affected() == 1)
}

/// Turns on the enrolment of `user_id`, its first code accepted at `step`;
/// whether it was waiting.
pub async fn enable_second_factor(
    conn: &mut PgConnection,
    user_id: Uuid,
    step: i64,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "update second_factors set enabled_at = now(), last_step = $2, failed_attempts = 0 \
         where user_id = $1 and enabled_at is null",
    )
    .bind(user_id)
    .bind(step)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Records a code accepted at `step`, only past the last one: whether it
/// was (a concurrent use of the same code finds it taken).
pub async fn accept_step(conn: &mut PgConnection, user_id: Uuid, step: i64) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "update second_factors set last_step = $2, failed_attempts = 0 \
         where user_id = $1 and enabled_at is not null and last_step < $2",
    )
    .bind(user_id)
    .bind(step)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Counts a code refused; the codes refused in a row since, if the account
/// has a second factor on.
pub async fn count_refusal(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<Option<i32>> {
    sqlx::query_scalar(
        "update second_factors set failed_attempts = failed_attempts + 1 \
         where user_id = $1 and enabled_at is not null returning failed_attempts",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// Replaces the recovery codes of `user_id` by these digests: the former
/// ones stop working.
pub async fn replace_recovery_codes(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    digests: &[Vec<u8>],
) -> sqlx::Result<()> {
    sqlx::query("delete from second_factor_recovery_codes where user_id = $1")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "insert into second_factor_recovery_codes (tenant_id, user_id, code_hash) \
         select $1, $2, unnest($3::bytea[])",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(digests)
    .execute(conn)
    .await?;
    Ok(())
}

/// Uses the recovery code of `user_id` whose digest is `digest`, once:
/// whether it was still unused. It ends the refusals in a row.
pub async fn use_recovery_code(
    conn: &mut PgConnection,
    user_id: Uuid,
    digest: &[u8],
) -> sqlx::Result<bool> {
    let used = sqlx::query(
        "update second_factor_recovery_codes set used_at = now() \
         where user_id = $1 and code_hash = $2 and used_at is null",
    )
    .bind(user_id)
    .bind(digest)
    .execute(&mut *conn)
    .await?;
    if used.rows_affected() != 1 {
        return Ok(false);
    }
    sqlx::query("update second_factors set failed_attempts = 0 where user_id = $1")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(true)
}

/// Deletes the second factor of `user_id`, its recovery codes with it;
/// whether there was one on (an enrolment that waited goes too).
pub async fn delete_second_factor(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<bool> {
    let enabled: Option<bool> = sqlx::query_scalar(
        "delete from second_factors where user_id = $1 returning enabled_at is not null",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(enabled == Some(true))
}
