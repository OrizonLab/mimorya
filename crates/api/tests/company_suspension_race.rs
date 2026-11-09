//! A company whose status another command changes while the operator's
//! suspension waits on its row (ADR-191): the suspension, which had read it
//! active, finds it suspended once that change commits and is refused
//! unchanged, with no audit entry and nothing announced of its own.

mod support;

use std::time::Duration;

use mimorya_api::services::provisioning::{CompanyStatusChange, change_company_status};
use mimorya_core::{DomainError, ErrorCode};
use sqlx::PgPool;
use support::TestApi;
use support::companies::create;
use support::owner_url;
use support::suspension::state;

const SLUG: &str = "acme-entre-temps";

/// Whether a statement waits on a lock, within five seconds.
async fn one_waits_on_a_lock(owner: &PgPool) -> bool {
    for _ in 0..100 {
        // Every role reads `pg_locks`; another's wait events need more.
        let waiting: i64 = sqlx::query_scalar("select count(*) from pg_locks where not granted")
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
async fn a_company_changed_meanwhile_is_refused_unchanged() {
    let api = TestApi::start().await;
    let created = create(&api, SLUG, "jeanne.martin@acme-entre-temps.example").await;
    let owner = PgPool::connect(owner_url()).await.unwrap();

    // Another command holds the company's row and suspends it, uncommitted.
    let mut other = owner.begin().await.unwrap();
    sqlx::query("select id from tenants where id = $1 for update")
        .bind(created.tenant_id)
        .execute(&mut *other)
        .await
        .unwrap();
    sqlx::query("update tenants set status = 'suspended' where id = $1")
        .bind(created.tenant_id)
        .execute(&mut *other)
        .await
        .unwrap();
    let db = api.app.services.db.clone();
    let suspension = tokio::spawn(async move {
        change_company_status(&db, SLUG, CompanyStatusChange::Suspend).await
    });
    // It read the company active, and now waits on its row.
    assert!(one_waits_on_a_lock(&owner).await);
    other.commit().await.unwrap();

    let refusal = suspension
        .await
        .unwrap()
        .expect_err("refused")
        .downcast::<DomainError>()
        .expect("domain error");
    assert_eq!(refusal.code, ErrorCode::Conflict);
    assert_eq!(refusal.message, "Cette entreprise est déjà suspendue");
    let (status, entries) = state(&api, created.tenant_id, "tenant.suspended").await;
    assert_eq!((status.as_str(), entries), ("suspended", 0));
    owner.close().await;
}
