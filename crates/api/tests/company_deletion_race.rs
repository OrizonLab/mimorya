//! A suspended company that another command reactivates while the
//! operator's deletion waits on its row (ADR-192): the deletion, which locks
//! the company's row before reading its status, reads it active once that
//! reactivation commits and is refused, deleting nothing. Without the lock
//! it would have read it suspended and deleted a company back in use.

mod support;

use std::time::Duration;

use mimorya_api::services::provisioning::{CompanyStatusChange, delete_company};
use mimorya_core::{DomainError, ErrorCode};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use support::TestApi;
use support::companies::{create, rows_of};
use support::owner_url;
use support::retention::owner;
use support::suspension::change;

const SLUG: &str = "acme-reprise";
/// The deletion's own connection, told from any other by its name.
const DELETION: &str = "mimorya-deletion-race";

/// Whether the deletion's connection waits on a lock, within five seconds.
async fn the_deletion_waits_on_a_lock(owner: &PgPool) -> bool {
    for _ in 0..100 {
        let waiting: i64 = sqlx::query_scalar(
            "select count(*) from pg_locks l join pg_stat_activity a on a.pid = l.pid \
             where not l.granted and a.application_name = $1",
        )
        .bind(DELETION)
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_reactivated_meanwhile_is_not_deleted() {
    let api = TestApi::start().await;
    let created = create(&api, SLUG, "jeanne.martin@acme-reprise.example").await;
    change(&api, SLUG, CompanyStatusChange::Suspend)
        .await
        .unwrap();
    let owner = owner().await;
    let held = rows_of(&owner, created.tenant_id).await;

    // Another command holds the company's row and reactivates it, uncommitted.
    let mut other = owner.begin().await.unwrap();
    sqlx::query("select id from tenants where id = $1 for update")
        .bind(created.tenant_id)
        .execute(&mut *other)
        .await
        .unwrap();
    sqlx::query("update tenants set status = 'active' where id = $1")
        .bind(created.tenant_id)
        .execute(&mut *other)
        .await
        .unwrap();
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
    assert!(the_deletion_waits_on_a_lock(&owner).await);
    other.commit().await.unwrap();

    let refusal = deletion
        .await
        .unwrap()
        .expect_err("refused")
        .downcast::<DomainError>()
        .expect("domain error");
    assert_eq!(refusal.code, ErrorCode::Conflict);
    assert!(
        refusal
            .message
            .starts_with("Seule une entreprise suspendue"),
        "{}",
        refusal.message
    );
    let status: String = sqlx::query_scalar("select status from tenants where id = $1")
        .bind(created.tenant_id)
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(status, "active");
    assert_eq!(rows_of(&owner, created.tenant_id).await, held);
    owner.close().await;
}
