//! The migration that brings the model supply formulas (0095, ADR-208) on
//! companies that already exist: each one on the platform's model keeps it,
//! capped at 100 USD a month or at twice what it spent this month if more,
//! its past calls counted under the capped formula at their real cost, but
//! the reviews of a model it chose apart and its embeddings; a company with
//! its own provider stays on its own key. Played on a database of its own,
//! migrated up to the one before, then to the last.

use std::borrow::Cow;
use std::str::FromStr;

use mimorya_db::migrate::MIGRATOR;
use mimorya_db::testing::test_database_urls;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection, PgPool};
use uuid::Uuid;

const BEFORE: i64 = 94;

/// The database of the test's own, beside the shared one.
fn database_name() -> String {
    format!("{}_supply_migration_test", test_database_urls().database)
}

/// Drops the test's own database, if it is there.
async fn drop_database(options: &PgConnectOptions) {
    let mut admin = options
        .clone()
        .database("postgres")
        .connect()
        .await
        .unwrap();
    sqlx::query(&format!(
        "drop database if exists \"{}\" with (force)",
        database_name()
    ))
    .execute(&mut admin)
    .await
    .unwrap();
    admin.close().await.unwrap();
}

/// The test's own database, created afresh: its URL.
async fn fresh_database() -> String {
    let options = PgConnectOptions::from_str(&test_database_urls().owner).unwrap();
    drop_database(&options).await;
    let mut admin = options
        .clone()
        .database("postgres")
        .connect()
        .await
        .unwrap();
    sqlx::query(&format!("create database \"{}\"", database_name()))
        .execute(&mut admin)
        .await
        .unwrap();
    admin.close().await.unwrap();
    options
        .database(&database_name())
        .to_url_lossy()
        .to_string()
}

async fn tenant(pool: &PgPool, slug: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("insert into tenants (id, slug, name) values ($1, $2, $2)")
        .bind(id)
        .bind(slug)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn own_provider(pool: &PgPool, tenant: Uuid, purpose: &str) {
    sqlx::query(
        "insert into tenant_llm_settings (tenant_id, purpose, provider, model) \
         values ($1, $2, 'anthropic', 'claude-haiku-5-5')",
    )
    .bind(tenant)
    .bind(purpose)
    .execute(pool)
    .await
    .unwrap();
}

/// A call of this month that cost `cost_micros`.
async fn spent(pool: &PgPool, tenant: Uuid, kind: &str, role: &str, cost_micros: i64) {
    sqlx::query(
        "insert into usage_records (tenant_id, kind, role, provider, model, cost_micros) \
         values ($1, $2, $3, 'anthropic', 'claude-haiku-5-5', $4)",
    )
    .bind(tenant)
    .bind(kind)
    .bind(role)
    .bind(cost_micros)
    .execute(pool)
    .await
    .unwrap();
}

async fn supply(pool: &PgPool, tenant: Uuid) -> Option<(String, Option<i64>, bool)> {
    sqlx::query_as(
        "select formula, monthly_cap_micros, unlimited from tenant_model_supply \
         where tenant_id = $1",
    )
    .bind(tenant)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// What each call of `tenant` became: its role, kind, formula and amount
/// re-invoiced, beside its real cost.
async fn charged(pool: &PgPool, tenant: Uuid) -> Vec<(String, String, String, Option<i64>, i64)> {
    sqlx::query_as(
        "select role, kind, supply_formula, billed_micros, cost_micros from usage_records \
         where tenant_id = $1 order by id",
    )
    .bind(tenant)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn existing_companies_keep_their_access_capped_or_their_own_key() {
    let url = fresh_database().await;
    let pool = PgPool::connect(&url).await.unwrap();
    // The migrations up to the one before (fields `sqlx::migrate!` sets).
    let before = Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .migrations
                .iter()
                .filter(|m| m.version <= BEFORE)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    before.run(&pool).await.unwrap();

    // On the platform's model, 80 USD spent this month, a review by a model
    // chosen apart and an embedding besides.
    let busy = tenant(&pool, "occupee").await;
    spent(&pool, busy, "llm", "main", 50_000_000).await;
    spent(&pool, busy, "llm", "main", 30_000_000).await;
    own_provider(&pool, busy, "verification").await;
    spent(&pool, busy, "llm", "review", 4_000_000).await;
    spent(&pool, busy, "embedding", "main", 1_000).await;
    // On the platform's model, nothing spent.
    let quiet = tenant(&pool, "calme").await;
    // On its own provider.
    let own = tenant(&pool, "autonome").await;
    own_provider(&pool, own, "conversations").await;
    spent(&pool, own, "llm", "main", 70_000_000).await;

    MIGRATOR.run(&pool).await.unwrap();

    assert_eq!(
        supply(&pool, busy).await,
        Some(("capped".into(), Some(160_000_000), false))
    );
    assert_eq!(
        supply(&pool, quiet).await,
        Some(("capped".into(), Some(100_000_000), false))
    );
    assert_eq!(supply(&pool, own).await, None);
    let row = |role: &str, kind: &str, formula: &str, billed: Option<i64>, cost: i64| {
        (role.into(), kind.into(), formula.into(), billed, cost)
    };
    assert_eq!(
        charged(&pool, busy).await,
        [
            row("main", "llm", "capped", Some(50_000_000), 50_000_000),
            row("main", "llm", "capped", Some(30_000_000), 30_000_000),
            row("review", "llm", "client_key", None, 4_000_000),
            row("main", "embedding", "client_key", None, 1_000),
        ]
    );
    assert_eq!(
        charged(&pool, own).await,
        [row("main", "llm", "client_key", None, 70_000_000)]
    );
    pool.close().await;
    drop_database(&PgConnectOptions::from_str(&test_database_urls().owner).unwrap()).await;
}
