//! A former start's rate limit windows, forgotten as the server starts: the
//! suite runs again within the hour, and a restart forgot them when they
//! were counted in memory (ADR-088). The application role can no longer
//! purge a window still counting (ADR-152): the database's owner, whom the
//! test environment names (`DATABASE_URL`), forgets them.

use anyhow::Context;

pub async fn forget() -> anyhow::Result<()> {
    let owner_url = std::env::var("DATABASE_URL").context("DATABASE_URL: the database's owner")?;
    let owner = sqlx::PgPool::connect(&owner_url).await?;
    sqlx::query("delete from rate_limit_windows")
        .execute(&owner)
        .await?;
    owner.close().await;
    Ok(())
}
