//! The audit log kept five years, its verification with it (DATA_MODEL §5,
//! ADR-198). A pass deletes the oldest places of each company's chain, a
//! prefix of it alone: five years old each, one after the other, never its
//! last sealed place nor an entry still to seal; a recent entry or a
//! missing place stops it. The chain is then verified from where it
//! starts, from the seal the purge kept; an anchor before it is said to be
//! gone with it, one at its eve checked against that seal. The company
//! finds the purge in its audit log, sealed in turn; the sealing goes on,
//! a process that remembered a place gone with the purge too. The
//! application writes no chain start. The tests start a process without
//! maintenance: they run the passes and the sealing themselves.

mod support;

use mimorya_api::plugins::retention::apply_retention;
use mimorya_api::services::audit_chain::{AnchorFound, ChainBreak, SealedHeads, verify_company};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::retention::purge_old_audit_entries;
use serde_json::json;
use support::TestApi;
use support::audit_chain::{as_owner, key, seal, verify};
use support::old_audit::{empty_chain, entries, place, places, purges};
use support::queue::API;
use uuid::Uuid;

/// What the purge deleted of the company `tenant_id`: entries, through.
async fn purged(api: &TestApi, tenant_id: Uuid) -> Option<(i32, i64)> {
    let lost = purge_old_audit_entries(api.app.services.db.pool(), 5000)
        .await
        .unwrap();
    lost.iter()
        .find(|company| company.tenant_id == tenant_id)
        .map(|company| (company.entries, company.through_seq))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_oldest_places_go_the_chain_verified_from_its_start() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let heads = SealedHeads::default();
    let acme = empty_chain(&api, "acme-journal-ancien").await;
    entries(db, acme, "ancienne.action", 5, 6).await;
    entries(db, acme, "recente.action", 3, 0).await;
    assert_eq!(seal(db, &heads, acme).await.unwrap(), 8);
    let anchors = [
        place(db, acme, 2).await,
        place(db, acme, 5).await,
        place(db, acme, 7).await,
    ];

    // The five entries six years old go; the company is told.
    let retained = apply_retention(db).await.unwrap().unwrap();
    let lost = retained.audit.iter().find(|lost| lost.tenant_id == acme);
    assert_eq!(
        lost.map(|lost| (lost.entries, lost.through_seq)),
        Some((5, 5))
    );
    assert_eq!(places(db, acme).await, [6, 7, 8]);
    assert_eq!(
        purges(db, acme).await,
        [json!({ "entries": 5, "throughPlace": 5 })]
    );

    // Sealed in turn, the purge's own entry with the rest, by the process
    // that sealed before: the chain holds from its new start.
    assert_eq!(seal(db, &heads, acme).await.unwrap(), 1);
    let report = verify_company(db, &key(0), acme, &anchors).await.unwrap();
    assert_eq!(report.broken, 0, "{report:?}");
    assert_eq!(report.verified, 4);
    let start = report.start.clone().unwrap();
    assert_eq!((start.first_seq, start.purged_entries), (6, 5));
    assert_eq!(start.previous_hash, anchors[1].chain_hash);
    assert_eq!(
        report.anchors,
        [AnchorFound::Purged, AnchorFound::Held, AnchorFound::Held]
    );
    assert!(report.holds());

    // An anchor at the start's eve with another seal does not hold.
    let mut forged = anchors[1].clone();
    forged.chain_hash = vec![0; 32];
    let report = verify_company(db, &key(0), acme, &[forged]).await.unwrap();
    assert_eq!(report.anchors, [AnchorFound::Differs]);
    assert!(!report.holds());

    // Nothing more five years old: nothing goes, no one told again.
    assert_eq!(purged(&api, acme).await, None);
    assert_eq!(purges(db, acme).await.len(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_recent_entry_a_missing_place_and_the_last_sealed_one_stop_it() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let heads = SealedHeads::default();

    // A recent entry among old ones: only those before it go.
    let mixed = empty_chain(&api, "acme-journal-mele").await;
    entries(db, mixed, "ancienne", 2, 6).await;
    entries(db, mixed, "recente", 1, 0).await;
    entries(db, mixed, "ancienne", 2, 6).await;
    seal(db, &heads, mixed).await.unwrap();
    assert_eq!(purged(&api, mixed).await, Some((2, 2)));
    assert_eq!(places(db, mixed).await, [3, 4, 5]);

    // A place missing among old ones: only those before it go, and the
    // verification still shows it.
    let gap = empty_chain(&api, "acme-journal-trou").await;
    entries(db, gap, "ancienne", 6, 6).await;
    seal(db, &heads, gap).await.unwrap();
    let third = entry_at(db, gap, 3).await;
    as_owner("delete from audit_log where id = $1", third)
        .await
        .unwrap();
    assert_eq!(purged(&api, gap).await, Some((2, 2)));
    assert_eq!(purged(&api, gap).await, None);
    let report = verify(db, gap).await;
    assert_eq!(report.breaks, [ChainBreak::Missing { from: 3, to: 3 }]);

    // All old: the last sealed place stays, and an entry still to seal.
    let old = empty_chain(&api, "acme-journal-vieux").await;
    entries(db, old, "ancienne", 3, 6).await;
    seal(db, &heads, old).await.unwrap();
    entries(db, old, "ancienne-a-sceller", 1, 6).await;
    assert_eq!(purged(&api, old).await, Some((2, 2)));
    assert_eq!(places(db, old).await, [3]);
    let report = verify(db, old).await;
    assert_eq!((report.verified, report.unsealed), (1, 2));
}

/// The entry at the place `seq` of the company's chain.
async fn entry_at(db: &mimorya_db::Database, tenant_id: Uuid, seq: i64) -> i64 {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let id = sqlx::query_scalar("select id from audit_log where chain_seq = $1")
        .bind(seq)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_sealing_goes_on_past_a_place_it_remembered_gone_with_the_purge() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let acme = empty_chain(&api, "acme-journal-tete").await;
    // One process seals the old entries, and remembers the third place;
    // another seals the recent ones after them.
    let (first, other) = (SealedHeads::default(), SealedHeads::default());
    entries(db, acme, "ancienne", 3, 6).await;
    seal(db, &first, acme).await.unwrap();
    entries(db, acme, "recente", 2, 0).await;
    seal(db, &other, acme).await.unwrap();

    // The place the first remembers goes with the old ones; it seals on.
    assert_eq!(purged(&api, acme).await, Some((3, 3)));
    entries(db, acme, "suivante", 1, 0).await;
    assert_eq!(seal(db, &first, acme).await.unwrap(), 2);
    assert_eq!(places(db, acme).await, [4, 5, 6, 7]);
    assert!(verify(db, acme).await.holds());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_application_writes_no_chain_start() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let heads = SealedHeads::default();
    let acme = empty_chain(&api, "acme-journal-debut").await;
    entries(db, acme, "ancienne", 2, 6).await;
    seal(db, &heads, acme).await.unwrap();
    assert_eq!(purged(&api, acme).await, Some((1, 1)));

    const DENIED: &str = "42501";
    for statement in [
        "update audit_chain_starts set first_seq = 1000",
        "delete from audit_chain_starts",
        "insert into audit_chain_starts (tenant_id, first_seq, previous_hash, purged_entries) \
         values (current_setting('app.tenant_id')::uuid, 9, '\\x00', 1)",
    ] {
        let mut tx = db.begin(acme).await.unwrap();
        let refused = sqlx::query(statement).execute(&mut *tx).await.unwrap_err();
        let code = refused.as_database_error().and_then(|error| error.code());
        assert_eq!(code.as_deref(), Some(DENIED), "{statement}");
    }

    // Each company reads its own start alone.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let seen: i64 =
        sqlx::query_scalar("select count(*) from audit_chain_starts where tenant_id = $1")
            .bind(acme)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(seen, 0);
}
