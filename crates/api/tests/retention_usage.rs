//! Retention applied to the consumption (DATA_MODEL §5, ADR-197). A pass
//! deletes the records older than five years, of every company, the oldest
//! of them all first, a batch at a time; younger ones stay. Each company
//! that lost records finds how many in its own audit log, by the platform;
//! a pass with nothing to delete tells no one; a backlog goes over several
//! statements of one pass. The horizon is the database's: the application
//! names none, and the database bounds the batch. A record another write holds (its account erased, its run
//! deleted) is skipped, never waited on, and taken by the next pass. The
//! tests start a process without maintenance: they run the passes
//! themselves.

mod support;

use std::time::Duration;

use mimorya_api::plugins::retention::apply_retention;
use mimorya_core::demo::tenant;
use mimorya_db::repositories::retention::{PurgedUsage, purge_old_usage};
use serde_json::json;
use support::queue::API;
use support::retention::{owner, purged_entries};
use support::usage_records::{usage_left, usage_record, usage_records};
use support::{TestApi, companies};
use uuid::Uuid;

const PURGED: &str = "usage.purged";

/// What a statement deleted, by company, sorted.
fn lost(batch: &[PurgedUsage]) -> Vec<(Uuid, i32)> {
    let mut lost: Vec<_> = batch
        .iter()
        .map(|company| (company.tenant_id, company.records))
        .collect();
    lost.sort();
    lost
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn consumption_older_than_five_years_goes_each_company_told() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let neo = tenant::ID;
    let acme =
        companies::create(&api, "acme-consommation", "admin@acme-consommation.example").await;
    usage_record(neo, "conso-neo-61-mois", 61).await;
    usage_record(neo, "conso-neo-61-mois-bis", 61).await;
    usage_record(neo, "conso-neo-59-mois", 59).await;
    usage_record(acme.tenant_id, "conso-acme-72-mois", 72).await;
    usage_record(acme.tenant_id, "conso-acme-1-mois", 1).await;
    let before_neo = purged_entries(neo, PURGED).await.len();

    // Past five years, every company's; within, none.
    let retained = apply_retention(db, None).await.unwrap().unwrap();
    let mut both = [(neo, 2), (acme.tenant_id, 1)];
    both.sort();
    assert_eq!(lost(&retained.usage), both);
    assert_eq!(
        usage_left("conso-").await,
        ["conso-acme-1-mois", "conso-neo-59-mois"]
    );

    // Each company told how many, by the platform, never what.
    let neo_entries = purged_entries(neo, PURGED).await;
    assert_eq!(neo_entries.len(), before_neo + 1);
    assert_eq!(neo_entries.last(), Some(&json!({ "records": 2 })));
    assert_eq!(
        purged_entries(acme.tenant_id, PURGED).await,
        [json!({ "records": 1 })]
    );

    // Nothing more to delete: no one told again.
    let again = apply_retention(db, None).await.unwrap().unwrap();
    assert!(again.usage.is_empty(), "{:?}", again.usage);
    assert_eq!(purged_entries(neo, PURGED).await.len(), before_neo + 1);
    assert_eq!(purged_entries(acme.tenant_id, PURGED).await.len(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_batch_takes_the_oldest_of_all_the_companies() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let neo = tenant::ID;
    let acme = companies::create(&api, "acme-lots-conso", "admin@acme-lots-conso.example").await;
    let acme_id = acme.tenant_id;
    // The oldest of all in the company read second: a batch in the
    // companies' order would take Néo's first.
    usage_record(acme_id, "lot-conso-acme-90-mois", 90).await;
    usage_record(neo, "lot-conso-neo-80-mois", 80).await;
    usage_record(acme_id, "lot-conso-acme-70-mois", 70).await;
    usage_record(neo, "lot-conso-neo-65-mois", 65).await;

    let one = purge_old_usage(db.pool(), 1).await.unwrap();
    assert_eq!(lost(&one), [(acme_id, 1)]);
    let mut both = [(neo, 1), (acme_id, 1)];
    both.sort();
    let two = purge_old_usage(db.pool(), 2).await.unwrap();
    assert_eq!(lost(&two), both);
    assert_eq!(usage_left("lot-conso-").await, ["lot-conso-neo-65-mois"]);
    // A batch asked for less than one still takes one.
    let none_asked = purge_old_usage(db.pool(), 0).await.unwrap();
    assert_eq!(lost(&none_asked), [(neo, 1)]);
    assert!(usage_left("lot-conso-").await.is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_batch_is_five_thousand_records_at_most() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    usage_records(tenant::ID, "borne-conso", 5001, 61).await;

    let asked_more = purge_old_usage(db.pool(), 10_000).await.unwrap();
    assert_eq!(lost(&asked_more), [(tenant::ID, 5000)]);
    let rest = purge_old_usage(db.pool(), 10_000).await.unwrap();
    assert_eq!(lost(&rest), [(tenant::ID, 1)]);
    assert!(usage_left("borne-conso").await.is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_record_another_write_holds_is_skipped_then_taken() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    usage_record(tenant::ID, "verrou-conso-tenu", 61).await;
    usage_record(tenant::ID, "verrou-conso-libre", 61).await;

    // An erasure or a run's deletion holds one, as it empties what it
    // names.
    let owner = owner().await;
    let mut hold = owner.begin().await.unwrap();
    sqlx::query("select id from usage_records where model = 'verrou-conso-tenu' for update")
        .fetch_all(&mut *hold)
        .await
        .unwrap();

    // The purge waits on nothing: the free one goes, the held one stays.
    let batch = tokio::time::timeout(Duration::from_secs(10), purge_old_usage(db.pool(), 10))
        .await
        .expect("the purge waited on a held record")
        .unwrap();
    assert_eq!(lost(&batch), [(tenant::ID, 1)]);
    assert_eq!(usage_left("verrou-conso").await, ["verrou-conso-tenu"]);

    // Released, the next takes it.
    hold.commit().await.unwrap();
    let next = purge_old_usage(db.pool(), 10).await.unwrap();
    assert_eq!(lost(&next), [(tenant::ID, 1)]);
    assert!(usage_left("verrou-conso").await.is_empty());
    owner.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pass_takes_a_backlog_over_several_statements() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    // More than a statement takes: the pass goes on while one comes back
    // full, which a batch the database cuts short would end.
    usage_records(tenant::ID, "arriere-conso", 7000, 61).await;
    let retained = apply_retention(db, None).await.unwrap().unwrap();
    assert_eq!(lost(&retained.usage), [(tenant::ID, 7000)]);
    assert!(usage_left("arriere-conso").await.is_empty());
}
