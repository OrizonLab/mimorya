//! A company deleted by the operator while another command changes its
//! status (ADR-192), both waiting on its row in turn. A deletion waiting
//! on a reactivation reads the company active once that commits, and is
//! refused, deleting nothing: without the lock it takes before reading the
//! status, it would have deleted a company back in use. A reactivation
//! waiting on a deletion finds no company once that commits, and says so.

mod support;

use std::time::Duration;

use mimorya_api::services::provisioning::{
    CompanyStatusChange, change_company_status, delete_company,
};
use mimorya_core::{DomainError, ErrorCode};
use mimorya_db::{Database, PoolOptions};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use support::TestApi;
use support::companies::{create, rows_of};
use support::retention::owner;
use support::suspension::change;
use support::{app_url, owner_url};

/// The deletion's own connection, told from any other by its name.
const DELETION: &str = "mimorya-deletion-race";
/// The reactivation's own connection.
const REACTIVATION: &str = "mimorya-reactivation-race";

/// Whether the connection named `name` waits on a lock, within five seconds.
async fn waits_on_a_lock(owner: &PgPool, name: &str) -> bool {
    for _ in 0..100 {
        let waiting: i64 = sqlx::query_scalar(
            "select count(*) from pg_locks l join pg_stat_activity a on a.pid = l.pid \
             where not l.granted and a.application_name = $1",
        )
        .bind(name)
        .fetch_one(owner)
        .await
        .unwrap();
        if waiting > 0 {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// A suspended company of its own, its slug `slug`.
async fn suspended(api: &TestApi, slug: &str) -> uuid::Uuid {
    let created = create(api, slug, &format!("jeanne.martin@{slug}.example")).await;
    change(api, slug, CompanyStatusChange::Suspend)
        .await
        .unwrap();
    created.tenant_id
}

/// Another command's transaction, holding the company's row and doing
/// `statement` to it, uncommitted.
async fn holding(
    owner: &PgPool,
    tenant_id: uuid::Uuid,
    statement: &str,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut other = owner.begin().await.unwrap();
    for statement in ["select id from tenants where id = $1 for update", statement] {
        sqlx::query(statement)
            .bind(tenant_id)
            .execute(&mut *other)
            .await
            .unwrap();
    }
    other
}

fn domain_error(done: anyhow::Result<impl std::fmt::Debug>) -> DomainError {
    done.expect_err("refused")
        .downcast::<DomainError>()
        .expect("domain error")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_reactivated_meanwhile_is_not_deleted() {
    const SLUG: &str = "acme-reprise";
    let api = TestApi::start().await;
    let tenant_id = suspended(&api, SLUG).await;
    let owner = owner().await;
    let held = rows_of(&owner, tenant_id).await;

    let other = holding(
        &owner,
        tenant_id,
        "update tenants set status = 'active' where id = $1",
    )
    .await;
    let deleting = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            owner_url()
                .parse::<sqlx::postgres::PgConnectOptions>()
                .unwrap()
                .application_name(DELETION),
        )
        .await
        .unwrap();
    let deletion = tokio::spawn(async move { delete_company(&deleting, SLUG, Some(SLUG)).await });
    // It waits on the company's row before reading anything of it.
    assert!(waits_on_a_lock(&owner, DELETION).await);
    other.commit().await.unwrap();

    let refusal = domain_error(deletion.await.unwrap());
    assert_eq!(refusal.code, ErrorCode::Conflict);
    assert!(
        refusal
            .message
            .starts_with("Seule une entreprise suspendue"),
        "{}",
        refusal.message
    );
    let status: String = sqlx::query_scalar("select status from tenants where id = $1")
        .bind(tenant_id)
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(status, "active");
    assert_eq!(rows_of(&owner, tenant_id).await, held);
    owner.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_deleted_meanwhile_is_not_reactivated() {
    const SLUG: &str = "acme-partie";
    let api = TestApi::start().await;
    let tenant_id = suspended(&api, SLUG).await;
    let owner = owner().await;

    // The deletion holds the company's row, and has deleted it, uncommitted.
    let deletion = holding(&owner, tenant_id, "delete from tenants where id = $1").await;
    let db = Database::connect(
        app_url(),
        PoolOptions {
            max_connections: 1,
            application_name: REACTIVATION,
        },
    )
    .await
    .unwrap();
    let reactivation = tokio::spawn(async move {
        change_company_status(&db, SLUG, CompanyStatusChange::Reactivate).await
    });
    // It read the company suspended, and now waits on its row.
    assert!(waits_on_a_lock(&owner, REACTIVATION).await);
    deletion.commit().await.unwrap();

    let refusal = domain_error(reactivation.await.unwrap());
    assert_eq!(refusal.code, ErrorCode::NotFound);
    assert_eq!(refusal.message, "Aucune entreprise ne porte ce sigle");
    assert_eq!(rows_of(&owner, tenant_id).await, Vec::new());
    owner.close().await;
}
