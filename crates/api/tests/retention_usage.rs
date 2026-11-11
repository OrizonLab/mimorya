//! Retention applied to the consumption (DATA_MODEL §5, ADR-197). A pass
//! deletes the records older than five years, of every company, the oldest
//! of them all first, a batch at a time; younger ones stay. Each company
//! that lost records finds how many in its own audit log, by the platform;
//! a pass with nothing to delete tells no one. The horizon is the
//! database's: the application names none. The tests start a process
//! without maintenance: they run the passes themselves.

mod support;

use mimorya_api::plugins::retention::apply_retention;
use mimorya_core::demo::tenant;
use mimorya_db::repositories::retention::{PurgedUsage, purge_old_usage};
use serde_json::json;
use support::queue::API;
use support::retention::purged_entries;
use support::usage_records::{usage_left, usage_record};
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
    let retained = apply_retention(db).await.unwrap().unwrap();
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
    let again = apply_retention(db).await.unwrap().unwrap();
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
