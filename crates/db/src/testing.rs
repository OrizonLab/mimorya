//! Helpers for integration tests that need a real PostgreSQL database. The
//! test database is dropped and recreated by `reset_test_database()`.

use std::str::FromStr;

use serde_json::json;
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection};
use uuid::Uuid;

use crate::client::{Database, PoolOptions};
use crate::migrate::run_migrations;
use crate::seed::{SeedOptions, seed_demo_tenant};

#[derive(Debug, Clone)]
pub struct TestDatabaseUrls {
    /// Owner role (migrations, seed).
    pub owner: String,
    /// Application role (row-level security applies).
    pub app: String,
    pub database: String,
}

pub fn test_database_urls() -> TestDatabaseUrls {
    let owner = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mimorya:mimorya@localhost:5432/mimorya_test".into());
    let app = std::env::var("TEST_DATABASE_APP_URL").unwrap_or_else(|_| {
        "postgres://mimorya_app:mimorya_app@localhost:5432/mimorya_test".into()
    });
    let options = PgConnectOptions::from_str(&owner).expect("valid TEST_DATABASE_URL");
    let database = options.get_database().unwrap_or_default().to_string();
    assert!(
        database.ends_with("_test"),
        "Refusing to reset \"{database}\": test database names must end with _test"
    );
    TestDatabaseUrls {
        owner,
        app,
        database,
    }
}

/// A second, unrelated tenant used by isolation tests.
pub const OTHER_TENANT_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_000f_0001);
pub const OTHER_USER_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_000f_0002);

pub async fn reset_test_database(
    password_hash: Option<String>,
) -> anyhow::Result<TestDatabaseUrls> {
    let urls = test_database_urls();
    let admin = PgConnectOptions::from_str(&urls.owner)?.database("postgres");
    let mut conn = admin.connect().await?;
    sqlx::query(&format!(
        "drop database if exists \"{}\" with (force)",
        urls.database
    ))
    .execute(&mut conn)
    .await?;
    sqlx::query(&format!("create database \"{}\"", urls.database))
        .execute(&mut conn)
        .await?;
    conn.close().await?;

    run_migrations(&urls.owner).await?;

    let db = Database::connect(
        &urls.owner,
        PoolOptions {
            max_connections: 2,
            application_name: "mimorya-test",
        },
    )
    .await?;
    seed_demo_tenant(
        &db,
        &SeedOptions {
            password_hash: password_hash.clone(),
            reset: false,
        },
    )
    .await?;

    let mut tx = db.begin(OTHER_TENANT_ID).await?;
    sqlx::query(
        "insert into tenants (id, slug, name) values ($1, 'autre-entreprise', 'Autre entreprise')",
    )
    .bind(OTHER_TENANT_ID)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "insert into users (id, tenant_id, email, display_name, password_hash) \
         values ($1, $2, 'contact@autre-entreprise.example', 'Contact Autre', $3)",
    )
    .bind(OTHER_USER_ID)
    .bind(OTHER_TENANT_ID)
    .bind(password_hash)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "insert into memory_items (tenant_id, scope, kind, title, content, source) \
         values ($1, 'tenant', 'fact', 'Secret commercial', $2, $3)",
    )
    .bind(OTHER_TENANT_ID)
    .bind("Tarif confidentiel négocié avec le client Alpha : 42 € / heure.")
    .bind(json!({ "type": "manual" }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    db.close().await;
    Ok(urls)
}
