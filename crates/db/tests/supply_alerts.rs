//! The alerts of the operator's supply (ADR-211) as the database keeps
//! them: the shares are the operator's, kept within their bounds, which the
//! application only reads; the alerts sent are written by the application
//! for its own company alone, read by it alone; and the companies the pass
//! reads are those on the operator's key, still active.

mod support;

use mimorya_core::demo::tenant;
use mimorya_db::Database;
use mimorya_db::repositories::supply_alerts::{
    record_supply_alerts, sent_supply_alerts, supplied_companies,
};
use mimorya_db::testing::OTHER_TENANT_ID;
use sqlx::PgPool;
use support::db_error;
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

async fn owner() -> PgPool {
    PgPool::connect(&support::urls(|_| {}).owner).await.unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_shares_are_the_operator_s_within_their_bounds() {
    let owner = owner().await;
    for (column, shares) in [
        ("cap_alert_percents", "{0}"),
        ("cap_alert_percents", "{101}"),
        ("cap_alert_percents", "{80,NULL}"),
        ("cap_alert_percents", "{10,20,30,40,50,60}"),
        ("credit_alert_percents", "{100}"),
        ("credit_alert_percents", "{-1}"),
    ] {
        let statement = format!(
            "update tenant_model_supply set {column} = $2::smallint[] where tenant_id = $1"
        );
        let refused = db_error(
            sqlx::query(&statement)
                .bind(tenant::ID)
                .bind(shares)
                .execute(&owner)
                .await,
        );
        assert!(
            refused.contains("tenant_model_supply_"),
            "{shares}: {refused}"
        );
    }
    // The application reads them, and may not write them.
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let refused = db_error(
        sqlx::query(
            "update tenant_model_supply set cap_alert_percents = '{100}' where tenant_id = $1",
        )
        .bind(tenant::ID)
        .execute(&mut *tx)
        .await,
    );
    assert!(refused.contains("permission denied"), "{refused}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_alerts_sent_are_each_company_s_own() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    record_supply_alerts(&mut tx, tenant::ID, "cap", "2026-11", &[80, 95])
        .await
        .unwrap();
    // Recorded twice, kept once.
    record_supply_alerts(&mut tx, tenant::ID, "cap", "2026-11", &[95])
        .await
        .unwrap();
    let mut sent = sent_supply_alerts(&mut tx, "cap", "2026-11").await.unwrap();
    sent.sort_unstable();
    assert_eq!(sent, [80, 95]);
    // Another company's: neither written from this one, nor seen.
    let refused =
        db_error(record_supply_alerts(&mut tx, OTHER_TENANT_ID, "cap", "2026-11", &[80]).await);
    assert!(refused.contains("row-level security"), "{refused}");
    tx.commit().await.unwrap();
    let mut other = db.begin(OTHER_TENANT_ID).await.unwrap();
    assert!(
        sent_supply_alerts(&mut other, "cap", "2026-11")
            .await
            .unwrap()
            .is_empty()
    );
    drop(other);
    let owner = owner().await;
    sqlx::query("delete from tenant_supply_alerts where tenant_id = $1")
        .bind(tenant::ID)
        .execute(&owner)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_pass_reads_the_active_companies_on_the_operator_s_key() {
    let owner = owner().await;
    let mut companies = Vec::new();
    for (slug, formula, status) in [
        ("alertes-plafond", "capped", "active"),
        ("alertes-credit", "prepaid", "active"),
        ("alertes-cle", "client_key", "active"),
        ("alertes-suspendue", "prepaid", "suspended"),
    ] {
        let id = Uuid::new_v4();
        sqlx::query("insert into tenants (id, slug, name, status) values ($1, $2, $2, $3)")
            .bind(id)
            .bind(slug)
            .bind(status)
            .execute(&owner)
            .await
            .unwrap();
        let cap = (formula == "capped").then_some(1_000_000_i64);
        sqlx::query(
            "insert into tenant_model_supply (tenant_id, formula, monthly_cap_micros) \
             values ($1, $2, $3)",
        )
        .bind(id)
        .bind(formula)
        .bind(cap)
        .execute(&owner)
        .await
        .unwrap();
        companies.push(id);
    }
    let read = supplied_companies(app().await.pool()).await.unwrap();
    let ours: Vec<bool> = companies.iter().map(|id| read.contains(id)).collect();
    assert_eq!(ours, [true, true, false, false]);
    sqlx::query("delete from tenants where id = any($1)")
        .bind(&companies)
        .execute(&owner)
        .await
        .unwrap();
}
