//! The authentication limits shared through the database (ADR-088): a test
//! starts with none still counting, as a fresh process did when they lived
//! in its memory, and may read or age what the database keeps of them, or
//! leave it unable to count.

use super::owner_url;

/// Forgets the windows still counting (opened within the hour): run as
/// each test app starts. Older ones are left for the maintenance to purge.
pub async fn forget_counts() {
    owner_query("delete from rate_limit_windows where window_start > now() - interval '1 hour'")
        .await;
}

/// Moves every window back by `hours`, as time would.
pub async fn age_windows(hours: i32) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "update rate_limit_windows set window_start = window_start - make_interval(hours => $1)",
    )
    .bind(hours)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// What the database keeps of `limiter`'s windows: each key's digest and
/// its count.
pub async fn windows_of(limiter: &str) -> Vec<(Vec<u8>, i32)> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let windows = sqlx::query_as(
        "select key_digest, hits from rate_limit_windows where limiter = $1 order by hits",
    )
    .bind(limiter)
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    windows
}

/// The digests of the values `limiter` counted, whatever their key.
pub async fn values_of(limiter: &str) -> Vec<Vec<u8>> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let values =
        sqlx::query_scalar("select value_digest from rate_limit_values where limiter = $1")
            .bind(limiter)
            .fetch_all(&owner)
            .await
            .unwrap();
    owner.close().await;
    values
}

/// Withdraws, or gives back, the application's right to count a hit: a
/// count it cannot read, as when the database fails it.
pub async fn allow_counting(allowed: bool) {
    let function = "function system_rate_limit_hit(text, bytea, integer, double precision)";
    let sql = if allowed {
        format!("grant execute on {function} to mimorya_app")
    } else {
        format!("revoke execute on {function} from mimorya_app")
    };
    owner_query(&sql).await;
}

async fn owner_query(sql: &str) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(sql).execute(&owner).await.unwrap();
    owner.close().await;
}
