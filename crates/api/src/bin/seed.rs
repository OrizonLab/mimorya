//! Seeds the FICTIONAL demo company. Development and demonstration only.
//!   DATABASE_URL   owner connection (migrations role)
//!   DEMO_PASSWORD  password of every demo account (required in production)
//!   --reset        recreates the demo company if it already exists

use mimorya_api::auth::credentials::hash_password_async;
use mimorya_db::seed::{SeedOptions, SeedOutcome, seed_demo_tenant};
use mimorya_db::{Database, PoolOptions, run_migrations};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL (owner role) is required"))?;
    let production = std::env::var("MIMORYA_ENV").as_deref() == Ok("production");
    let password = match std::env::var("DEMO_PASSWORD") {
        Ok(password) if !password.is_empty() => password,
        _ if production => anyhow::bail!("DEMO_PASSWORD is required in production"),
        _ => "demo-mimorya".to_string(),
    };

    run_migrations(&url).await?;
    let db = Database::connect(
        &url,
        PoolOptions {
            max_connections: 1,
            application_name: "mimorya-seed",
        },
    )
    .await?;
    let outcome = seed_demo_tenant(
        &db,
        &SeedOptions {
            password_hash: Some(hash_password_async(password).await?),
            reset: std::env::args().any(|arg| arg == "--reset"),
        },
    )
    .await;
    db.close().await;
    match outcome? {
        SeedOutcome::Created => println!("Demo tenant created"),
        SeedOutcome::Skipped => println!("Demo tenant already present (use --reset)"),
    }
    Ok(())
}
