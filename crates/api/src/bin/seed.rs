//! Seeds the FICTIONAL demo company. Development and demonstration only.
//!   DATABASE_URL   owner connection (migrations role)
//!   DEMO_PASSWORD  password of every demo account (required in production,
//!                  where the public development default is refused)
//!   --reset        recreates the demo company if it already exists

use mimorya_api::auth::credentials::hash_password_async;
use mimorya_db::seed::{SeedOptions, SeedOutcome, seed_demo_tenant};
use mimorya_db::{Database, PoolOptions, run_migrations};

/// The demo accounts' password when none is chosen, outside production. It is
/// public (documentation, end-to-end tests), hence refused in production.
const DEVELOPMENT_PASSWORD: &str = "demo-mimorya";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL (owner role) is required"))?;
    let production = std::env::var("MIMORYA_ENV").as_deref() == Ok("production");
    let password = demo_password(production, std::env::var("DEMO_PASSWORD").ok())?;

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

/// The chosen password; in production it must be chosen, and not be the
/// development default (which a copied `.env.example` once supplied).
fn demo_password(production: bool, chosen: Option<String>) -> anyhow::Result<String> {
    match chosen.filter(|password| !password.is_empty()) {
        Some(password) if production && password == DEVELOPMENT_PASSWORD => {
            anyhow::bail!("DEMO_PASSWORD must not be the public development default in production")
        }
        Some(password) => Ok(password),
        None if production => anyhow::bail!("DEMO_PASSWORD is required in production"),
        None => Ok(DEVELOPMENT_PASSWORD.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_falls_back_to_the_public_default() {
        assert_eq!(demo_password(false, None).unwrap(), DEVELOPMENT_PASSWORD);
        assert_eq!(
            demo_password(false, Some(String::new())).unwrap(),
            DEVELOPMENT_PASSWORD
        );
        assert_eq!(
            demo_password(false, Some("chosen".into())).unwrap(),
            "chosen"
        );
    }

    #[test]
    fn production_requires_a_chosen_password_other_than_the_default() {
        assert!(demo_password(true, None).is_err());
        assert!(demo_password(true, Some(String::new())).is_err());
        assert!(demo_password(true, Some(DEVELOPMENT_PASSWORD.into())).is_err());
        assert_eq!(
            demo_password(true, Some("chosen".into())).unwrap(),
            "chosen"
        );
    }
}
