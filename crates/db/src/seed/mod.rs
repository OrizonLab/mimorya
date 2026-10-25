//! Demo tenant provisioning (FICTIONAL data, see core `demo`).
//! Idempotent: an existing demo tenant is left untouched unless `reset` is set.

mod connectors;
mod documents;
mod memories;
mod organization;
mod skills;

use mimorya_core::demo::tenant;

use crate::client::Database;

pub use connectors::DEMO_CONNECTOR_ID;
pub use skills::DEMO_SKILL_ID;

#[derive(Debug, Clone, Default)]
pub struct SeedOptions {
    /// Argon2id hash applied to every demo account (`None`: accounts cannot log in).
    pub password_hash: Option<String>,
    pub reset: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedOutcome {
    Created,
    Skipped,
}

pub async fn seed_demo_tenant(db: &Database, options: &SeedOptions) -> anyhow::Result<SeedOutcome> {
    let mut tx = db.begin(tenant::ID).await?;
    let exists: bool = sqlx::query_scalar("select exists(select 1 from tenants where id = $1)")
        .bind(tenant::ID)
        .fetch_one(&mut *tx)
        .await?;
    if exists {
        if !options.reset {
            return Ok(SeedOutcome::Skipped);
        }
        sqlx::query("delete from tenants where id = $1")
            .bind(tenant::ID)
            .execute(&mut *tx)
            .await?;
    }
    organization::insert(&mut tx, options.password_hash.as_deref()).await?;
    connectors::insert(&mut tx).await?;
    memories::insert(&mut tx).await?;
    documents::insert(&mut tx).await?;
    skills::insert(&mut tx).await?;
    tx.commit().await?;
    Ok(SeedOutcome::Created)
}
