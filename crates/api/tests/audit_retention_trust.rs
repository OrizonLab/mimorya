//! Where an audit chain starts once its oldest places went (ADR-198), as
//! far as it can be trusted. A chain losing them over several statements
//! moves its start forward, each with its seal, its count summed, each
//! statement told. A start the database's owner writes or moves without
//! the key holds nothing: the verification finds the places before it
//! missing, the anchors there too; the sealing refuses to go on past a
//! place it remembered; the purge deletes nothing more. The age is judged
//! by the purging process's clock as much as the database's: neither takes
//! a place for older than the other does. The database moves no start
//! back, and refuses a purge it would not make, or made in another
//! company's transaction, the seals left to the process that holds the key. The tests start a process without
//! maintenance: they run the purges and the sealing themselves.

mod support;

use chrono::{Months, Utc};
use mimorya_api::services::audit_chain::{
    AnchorFound, ChainBreak, ChainMoved, MAX_PURGE_BATCH, SealedHeads, hex, verify_company,
};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::audit_chain::{PlacesPurged, purge_audit_places};
use serde_json::json;
use support::TestApi;
use support::audit_chain::{key, seal, verify};
use support::old_audit::{
    empty_chain, entries, owner_runs, place, places, purged_at, purged_with, purges, start_of,
};
use support::queue::API;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_chain_loses_its_old_places_over_several_statements() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let heads = SealedHeads::default();
    let acme = empty_chain(&api, "acme-journal-etapes").await;
    entries(db, acme, "ancienne", 5, 6).await;
    entries(db, acme, "recente", 1, 0).await;
    assert_eq!(seal(db, &heads, acme).await.unwrap(), 6);
    let eves = [place(db, acme, 2).await, place(db, acme, 4).await];

    // Two places a statement: the start moves forward, its count summed.
    assert_eq!(purged_with(db, acme, 2).await, Some((2, 2)));
    let first = start_of(db, acme).await.unwrap();
    assert_eq!((first.first_seq, first.purged_entries), (3, 2));
    assert_eq!(first.previous_hash, eves[0].chain_hash);
    assert_eq!(purged_with(db, acme, 2).await, Some((2, 4)));
    let second = start_of(db, acme).await.unwrap();
    assert_eq!((second.first_seq, second.purged_entries), (5, 4));
    assert_eq!(second.previous_hash, eves[1].chain_hash);
    assert_ne!(second.seal, first.seal);
    assert_eq!(places(db, acme).await, [5, 6]);
    assert_eq!(
        purges(db, acme).await,
        [
            json!({ "entries": 2, "throughPlace": 2 }),
            json!({ "entries": 2, "throughPlace": 4 })
        ]
    );

    // Sealed in turn, the chain holds from its new start: the former eve
    // gone with the places, the new one held.
    assert_eq!(seal(db, &heads, acme).await.unwrap(), 2);
    let report = verify_company(db, &key(0), acme, &eves).await.unwrap();
    assert_eq!(report.broken, 0, "{report:?}");
    assert_eq!(report.anchors, [AnchorFound::Purged, AnchorFound::Held]);
    assert_eq!(report.start.map(|start| start.first_seq), Some(5));

    // The last old place goes too, never the recent one after it.
    assert_eq!(purged_with(db, acme, 2).await, Some((1, 5)));
    assert_eq!(purged_with(db, acme, 2).await, None);
    assert_eq!(start_of(db, acme).await.unwrap().purged_entries, 5);
    assert!(verify(db, acme).await.broken == 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_start_written_or_moved_without_the_key_holds_nothing() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let acme = empty_chain(&api, "acme-journal-faux-debut").await;
    // One process seals the first three places, and remembers the third;
    // another the three after.
    let (first, other) = (SealedHeads::default(), SealedHeads::default());
    entries(db, acme, "ancienne", 3, 6).await;
    seal(db, &first, acme).await.unwrap();
    entries(db, acme, "ancienne", 3, 6).await;
    seal(db, &other, acme).await.unwrap();
    let anchors = [
        place(db, acme, 2).await,
        place(db, acme, 3).await,
        place(db, acme, 5).await,
    ];

    // The database's owner deletes the first three places and writes where
    // the chain would start, with the seal held at the third: without the
    // key, the start's own seal is not one.
    owner_runs(&format!(
        "delete from audit_log where tenant_id = '{acme}' and chain_seq <= 3"
    ))
    .await
    .unwrap();
    owner_runs(&format!(
        "insert into audit_chain_starts (tenant_id, first_seq, previous_hash, purged_entries, \
         seal) values ('{acme}', 4, '\\x{}', 3, '\\x{}')",
        hex(&anchors[1].chain_hash),
        "00".repeat(32)
    ))
    .await
    .unwrap();
    let report = verify_company(db, &key(0), acme, &anchors).await.unwrap();
    assert_eq!(report.start, None);
    assert_eq!(report.breaks, [ChainBreak::Missing { from: 1, to: 3 }]);
    assert_eq!(
        report.anchors,
        [
            AnchorFound::Missing,
            AnchorFound::Missing,
            AnchorFound::Held
        ]
    );
    assert!(!report.holds());
    // The purge deletes nothing more, the sealing goes no further.
    assert_eq!(purged_with(db, acme, 5000).await, None);
    assert_eq!(places(db, acme).await, [4, 5, 6]);
    entries(db, acme, "suivante", 1, 0).await;
    let refused = seal(db, &first, acme).await.unwrap_err();
    assert!(refused.is::<ChainMoved>(), "{refused:#}");

    // A start the purge sealed, moved forward by the owner with the place
    // after it: taken for none, the places before it missing.
    let moved = empty_chain(&api, "acme-journal-debut-deplace").await;
    entries(db, moved, "ancienne", 3, 6).await;
    entries(db, moved, "recente", 2, 0).await;
    seal(db, &SealedHeads::default(), moved).await.unwrap();
    assert_eq!(purged_with(db, moved, 5000).await, Some((3, 3)));
    let fourth = place(db, moved, 4).await;
    owner_runs(&format!(
        "update audit_chain_starts set first_seq = 5, purged_entries = 4, \
         previous_hash = '\\x{}' where tenant_id = '{moved}'",
        hex(&fourth.chain_hash)
    ))
    .await
    .unwrap();
    owner_runs(&format!(
        "delete from audit_log where tenant_id = '{moved}' and chain_seq = 4"
    ))
    .await
    .unwrap();
    let report = verify(db, moved).await;
    assert_eq!(report.start, None);
    assert_eq!(report.breaks[0], ChainBreak::Missing { from: 1, to: 4 });
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_start_only_moves_forward() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let acme = empty_chain(&api, "acme-journal-avance").await;
    entries(db, acme, "ancienne", 3, 6).await;
    seal(db, &SealedHeads::default(), acme).await.unwrap();
    assert_eq!(purged_with(db, acme, 5000).await, Some((2, 2)));

    // Not even the database's owner moves it back, or keeps it in place.
    for change in [
        "first_seq = first_seq - 1",
        "purged_entries = purged_entries - 1",
        "seal = seal",
    ] {
        let statement =
            format!("update audit_chain_starts set {change} where tenant_id = '{acme}'");
        let refused = owner_runs(&statement).await.unwrap_err();
        assert!(
            refused.to_string().contains("only moves forward"),
            "{change}: {refused}"
        );
    }
    assert_eq!(start_of(db, acme).await.unwrap().first_seq, 3);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_database_refuses_a_purge_it_would_not_make() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let acme = empty_chain(&api, "acme-journal-refus").await;
    entries(db, acme, "ancienne", 3, 6).await;
    entries(db, acme, "recente", 1, 0).await;
    entries(db, acme, "ancienne", 1, 6).await;
    seal(db, &SealedHeads::default(), acme).await.unwrap();
    let seal_at = |seq: i64| async move { place(db, acme, seq).await.chain_hash };
    let asked =
        |first: i64, through: i64, previous_hash: Vec<u8>, purged_entries: i64| PlacesPurged {
            first,
            through,
            previous_hash,
            purged_entries,
            seal: vec![7; 32],
        };

    let most = i64::from(MAX_PURGE_BATCH);
    let refusals = [
        // Its last sealed place, and a recent one.
        (asked(1, 5, seal_at(5).await, 5), "last sealed place stays"),
        (asked(1, 4, seal_at(4).await, 4), "not five years old"),
        // Not from where it starts.
        (asked(2, 3, seal_at(3).await, 2), "starts at place 1"),
        // Another seal kept, another count.
        (asked(1, 2, seal_at(1).await, 2), "seal kept"),
        (asked(1, 2, seal_at(2).await, 5), "places gone in all"),
        // A statement's most passes the database's bound, one more does
        // not, nor none: the platform's constant is the database's.
        (
            asked(1, most, seal_at(3).await, most),
            "last sealed place stays",
        ),
        (
            asked(1, most + 1, seal_at(3).await, most + 1),
            "places asked",
        ),
        (asked(3, 2, seal_at(2).await, 0), "places asked"),
    ];
    for (places_asked, why) in refusals {
        let mut tx = db.begin(acme).await.unwrap();
        let refused = purge_audit_places(&mut tx, acme, &places_asked)
            .await
            .unwrap_err();
        let code = refused.as_database_error().and_then(|error| error.code());
        assert_eq!(code.as_deref(), Some("P0001"), "{places_asked:?}");
        assert!(refused.to_string().contains(why), "{why}: {refused}");
    }
    assert_eq!(places(db, acme).await, [1, 2, 3, 4, 5]);

    // Asked from another company's transaction, or from none: refused, as
    // the sealing is.
    let fair = asked(1, 2, seal_at(2).await, 2);
    let mut other = db.begin(tenant::ID).await.unwrap();
    let foreign = purge_audit_places(&mut other, acme, &fair)
        .await
        .unwrap_err();
    assert!(foreign.to_string().contains("own company"), "{foreign}");
    drop(other);
    let mut bare = db.pool().acquire().await.unwrap();
    let unset = purge_audit_places(&mut bare, acme, &fair)
        .await
        .unwrap_err();
    assert!(unset.to_string().contains("own company"), "{unset}");
    drop(bare);
    assert_eq!(places(db, acme).await, [1, 2, 3, 4, 5]);
    assert_eq!(start_of(db, acme).await, None);

    // A gap among the places asked.
    owner_runs(&format!(
        "delete from audit_log where tenant_id = '{acme}' and chain_seq = 2"
    ))
    .await
    .unwrap();
    let mut tx = db.begin(acme).await.unwrap();
    let gap = purge_audit_places(&mut tx, acme, &asked(1, 3, seal_at(3).await, 3)).await;
    assert!(gap.is_err());
    drop(tx);
    assert_eq!(places(db, acme).await, [1, 3, 4, 5]);

    // What the database can check holds: it deletes, keeping the seal it is
    // given; one that no key made leaves the start taken for none.
    let mut tx = db.begin(acme).await.unwrap();
    let gone = purge_audit_places(&mut tx, acme, &asked(1, 1, seal_at(1).await, 1)).await;
    assert_eq!(gone.unwrap(), 1);
    tx.commit().await.unwrap();
    let report = verify(db, acme).await;
    assert_eq!(report.start, None);
    assert_eq!(report.breaks[0], ChainBreak::Missing { from: 1, to: 2 });
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn neither_clock_takes_a_place_for_older_than_the_other_does() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let acme = empty_chain(&api, "acme-journal-horloges").await;
    entries(db, acme, "ancienne", 2, 6).await;
    entries(db, acme, "quatre-ans", 2, 4).await;
    entries(db, acme, "recente", 1, 0).await;
    seal(db, &SealedHeads::default(), acme).await.unwrap();
    let years = |count: u32| Months::new(12 * count);

    // The process's clock two years behind the database's: the places six
    // years old are four to it, and stay.
    let behind = Utc::now().checked_sub_months(years(2)).unwrap();
    assert_eq!(purged_at(db, acme, 5000, behind).await, None);
    // Two years ahead: the places four years old are six to it, but not
    // to the database, whose horizon holds the purge to the oldest ones.
    let ahead = Utc::now().checked_add_months(years(2)).unwrap();
    assert_eq!(purged_at(db, acme, 5000, ahead).await, Some((2, 2)));
    assert_eq!(places(db, acme).await, [3, 4, 5]);
}
