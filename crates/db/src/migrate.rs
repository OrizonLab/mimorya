//! Schema migrations (SQL files in `migrations/`, embedded at build time).
//! They must run with the owner role, never with the application role.

use sqlx::migrate::Migrator;

use crate::client::{Database, PoolOptions};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn run_migrations(owner_url: &str) -> anyhow::Result<()> {
    let options = PoolOptions {
        max_connections: 1,
        application_name: "mimorya-migrate",
    };
    let db = Database::connect(owner_url, options).await?;
    let result = MIGRATOR.run(db.pool()).await;
    db.close().await;
    Ok(result?)
}
