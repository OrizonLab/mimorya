//! Rate limit windows shared by every process (ADR-088), through the narrowly
//! scoped database functions that alone touch them. The caller passes a
//! digest of each key and value, never the client address or e-mail address
//! itself; a window lasts `seconds`, on the database's clock.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Counts one hit of `key` for `limiter`; whether it is within `limit`.
pub async fn hit_rate_limit(
    pool: &PgPool,
    limiter: &str,
    key: &[u8],
    limit: i32,
    seconds: f64,
) -> sqlx::Result<bool> {
    sqlx::query_scalar("select system_rate_limit_hit($1, $2, $3, $4)")
        .bind(limiter)
        .bind(key)
        .bind(limit)
        .bind(seconds)
        .fetch_one(pool)
        .await
}

/// Gives back a hit of `key` allowed in the current window.
pub async fn release_rate_limit(
    pool: &PgPool,
    limiter: &str,
    key: &[u8],
    limit: i32,
    seconds: f64,
) -> sqlx::Result<()> {
    sqlx::query("select system_rate_limit_release($1, $2, $3, $4)")
        .bind(limiter)
        .bind(key)
        .bind(limit)
        .bind(seconds)
        .execute(pool)
        .await?;
    Ok(())
}

/// Counts `value` for `key` once a window; whether it is within `limit`
/// distinct values (one already counted always is).
pub async fn hit_distinct_rate_limit(
    pool: &PgPool,
    limiter: &str,
    key: &[u8],
    value: &[u8],
    limit: i32,
    seconds: f64,
) -> sqlx::Result<bool> {
    sqlx::query_scalar("select system_rate_limit_distinct($1, $2, $3, $4, $5)")
        .bind(limiter)
        .bind(key)
        .bind(value)
        .bind(limit)
        .bind(seconds)
        .fetch_one(pool)
        .await
}

/// Forgets the windows opened before `before`; how many.
pub async fn purge_rate_limits(pool: &PgPool, before: DateTime<Utc>) -> sqlx::Result<i64> {
    sqlx::query_scalar("select system_purge_rate_limits($1)")
        .bind(before)
        .fetch_one(pool)
        .await
}
