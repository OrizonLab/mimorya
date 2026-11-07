//! The audit log chained by company (ADR-178): sealed by the maintenance
//! of a server with a master key, each company in its own chain, verified
//! from its first place. An entry the database's owner changes or removes,
//! a seal rewritten, two places swapped or another key break the chain
//! where they are, and every break is counted, each missing place one. A
//! seal is never changed nor taken away, even by the owner, unless they
//! drop that rule first; a company whose log is sealed is still deleted.
//! What the database alone cannot show is in `audit_chain_guards.rs`.

mod support;

use std::time::Duration;

use mimorya_api::secrets::PUBLIC_TEST_MASTER_KEYS;
use mimorya_api::services::audit_chain::{ChainBreak, SealedHeads, seal_audit_log, verify_company};
use support::audit_chain::{
    altered, append, as_owner, as_owner_over_the_seals, chain, first_break, intact, key, seal,
    verify,
};
use support::{TestApi, companies, owner_url};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_is_sealed_in_its_own_chain_and_verified_intact() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let heads = SealedHeads::default();
    let acme = companies::create(&api, "chaine-acme", "jeanne@chaine-acme.example").await;
    let other = companies::create(&api, "chaine-autre", "jeanne@chaine-autre.example").await;
    for action in ["test.one", "test.two", "test.three"] {
        append(db, acme.tenant_id, action).await;
    }
    // Not before it is sealed.
    let before = verify(db, acme.tenant_id).await;
    assert_eq!((before.verified, before.broken), (0, 0));
    assert!(before.unsealed >= 3);

    assert!(seal_audit_log(db, &key(0), &heads).await.unwrap() >= 3);
    let acme_chain = chain(db, acme.tenant_id).await;
    let other_chain = chain(db, other.tenant_id).await;
    // Each company's places from 1, with no gap, in the order written.
    for places in [&acme_chain, &other_chain] {
        let seqs: Vec<i64> = places.iter().map(|(_, seq)| *seq).collect();
        assert_eq!(seqs, (1..=places.len() as i64).collect::<Vec<_>>());
        assert!(places.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }
    let report = verify(db, acme.tenant_id).await;
    intact(&report, acme_chain.len() as i64);
    assert_eq!(report.unsealed, 0);
    intact(&verify(db, other.tenant_id).await, other_chain.len() as i64);

    // The next entries follow the last place; one still to seal is counted.
    append(db, acme.tenant_id, "test.four").await;
    append(db, acme.tenant_id, "test.five").await;
    assert_eq!(seal(db, &heads, acme.tenant_id).await.unwrap(), 2);
    append(db, acme.tenant_id, "test.six").await;
    let report = verify(db, acme.tenant_id).await;
    intact(&report, acme_chain.len() as i64 + 2);
    assert_eq!(report.unsealed, 1);

    // Under another key, no seal holds, each counted.
    let other_key = verify_company(db, &key(1), acme.tenant_id, &[])
        .await
        .unwrap();
    assert_eq!(other_key.broken, acme_chain.len() as i64 + 2);
    assert_eq!(first_break(&other_key), altered((acme_chain[0].0, 1)));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_the_owner_changes_breaks_the_chain_where_it_is() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company =
        companies::create(&api, "chaine-falsifiee", "jeanne@chaine-falsifiee.example").await;
    for action in ["test.a", "test.b", "test.c", "test.d"] {
        append(db, company.tenant_id, action).await;
    }
    seal(db, &SealedHeads::default(), company.tenant_id)
        .await
        .unwrap();
    let places = chain(db, company.tenant_id).await;
    let count = places.len() as i64;
    let (third, fourth, last) = (places[2], places[3], places[places.len() - 1]);
    intact(&verify(db, company.tenant_id).await, count);

    // Each part of an entry's content, changed then put back.
    for (change, undo) in [
        (
            "update audit_log set action = action || '.falsifiée' where id = $1",
            "update audit_log set action = left(action, -10) where id = $1",
        ),
        (
            "update audit_log set metadata = metadata || '{\"ajout\": true}' where id = $1",
            "update audit_log set metadata = metadata - 'ajout' where id = $1",
        ),
        (
            "update audit_log set occurred_at = occurred_at + interval '1 microsecond' where id = $1",
            "update audit_log set occurred_at = occurred_at - interval '1 microsecond' where id = $1",
        ),
        (
            "update audit_log set ip = '198.51.100.1' where id = $1",
            "update audit_log set ip = '192.0.2.7' where id = $1",
        ),
    ] {
        as_owner(change, last.0).await.unwrap();
        let report = verify(db, company.tenant_id).await;
        assert_eq!(
            (first_break(&report), report.broken),
            (altered(last), 1),
            "{change}"
        );
        as_owner(undo, last.0).await.unwrap();
        intact(&verify(db, company.tenant_id).await, count);
    }

    // Two entries swapped places, past the rule that keeps the seals.
    let to_third = format!("update audit_log set chain_seq = {} where id = $1", third.1);
    let to_fourth = format!(
        "update audit_log set chain_seq = {} where id = $1",
        fourth.1
    );
    let aside = "update audit_log set chain_seq = 1000000 where id = $1";
    as_owner_over_the_seals(aside, third.0).await;
    as_owner_over_the_seals(&to_third, fourth.0).await;
    as_owner_over_the_seals(&to_fourth, third.0).await;
    let swapped = verify(db, company.tenant_id).await;
    assert_eq!(first_break(&swapped), altered((fourth.0, third.1)));
    assert!(swapped.broken >= 2, "{swapped:?}");
    as_owner_over_the_seals(aside, third.0).await;
    as_owner_over_the_seals(&to_fourth, fourth.0).await;
    as_owner_over_the_seals(&to_third, third.0).await;
    intact(&verify(db, company.tenant_id).await, count);

    // A seal rewritten without the key: it and the next one break.
    let rewrite = "update audit_log set chain_hash = sha256(chain_hash) where id = $1";
    as_owner_over_the_seals(rewrite, third.0).await;
    let rewritten = verify(db, company.tenant_id).await;
    assert_eq!(first_break(&rewritten), altered(third));
    assert_eq!(rewritten.breaks.get(1).cloned(), altered(fourth));
    assert_eq!(rewritten.broken, 2);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_seal_is_never_changed_nor_taken_away_even_by_the_owner() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company = companies::create(&api, "chaine-gardee", "jeanne@chaine-gardee.example").await;
    append(db, company.tenant_id, "test.kept").await;
    seal(db, &SealedHeads::default(), company.tenant_id)
        .await
        .unwrap();
    let places = chain(db, company.tenant_id).await;
    let first = places[0].0;
    for change in [
        "update audit_log set chain_seq = null, chain_hash = null where id = $1",
        "update audit_log set chain_hash = sha256(chain_hash) where id = $1",
        "update audit_log set chain_seq = chain_seq + 1000 where id = $1",
    ] {
        let refused = as_owner(change, first).await.unwrap_err().to_string();
        assert!(
            refused.contains("never changed nor taken away"),
            "{refused}"
        );
    }
    intact(&verify(db, company.tenant_id).await, places.len() as i64);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn entries_removed_leave_each_place_missing_and_the_others_verified() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company = companies::create(&api, "chaine-trouee", "jeanne@chaine-trouee.example").await;
    for action in ["test.a", "test.b", "test.c", "test.d", "test.e", "test.f"] {
        append(db, company.tenant_id, action).await;
    }
    seal(db, &SealedHeads::default(), company.tenant_id)
        .await
        .unwrap();
    let places = chain(db, company.tenant_id).await;
    let count = places.len() as i64;
    assert!(count > 6, "{places:?}");

    // One entry in the middle removed: its place is missing, the next entry
    // cannot be checked, the others still are.
    let (removed, seq) = places[2];
    as_owner("delete from audit_log where id = $1", removed)
        .await
        .unwrap();
    let report = verify(db, company.tenant_id).await;
    assert_eq!(report.breaks, [ChainBreak::Missing { from: seq, to: seq }]);
    assert_eq!((report.broken, report.verified), (1, count - 2));

    // Two more removed one after the other: each place is counted, the
    // range told once.
    for (removed, _) in &places[3..5] {
        as_owner("delete from audit_log where id = $1", *removed)
            .await
            .unwrap();
    }
    let report = verify(db, company.tenant_id).await;
    let range = ChainBreak::Missing {
        from: seq,
        to: seq + 2,
    };
    assert_eq!((report.breaks, report.broken), (vec![range], 3));

    // The first ones removed too: the chain is missing from its first place
    // to the end of the range.
    for (removed, _) in &places[..2] {
        as_owner("delete from audit_log where id = $1", *removed)
            .await
            .unwrap();
    }
    let report = verify(db, company.tenant_id).await;
    let from_the_start = ChainBreak::Missing {
        from: 1,
        to: seq + 2,
    };
    assert_eq!((report.breaks, report.broken), (vec![from_the_start], 5));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_whose_log_is_sealed_is_still_deleted() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company =
        companies::create(&api, "chaine-supprimee", "jeanne@chaine-supprimee.example").await;
    append(db, company.tenant_id, "test.sealed").await;
    seal(db, &SealedHeads::default(), company.tenant_id)
        .await
        .unwrap();
    assert!(!chain(db, company.tenant_id).await.is_empty());

    // The rule that keeps the seals bears on their columns only: the
    // company's deletion takes its chain with it.
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("delete from tenants where id = $1")
        .bind(company.tenant_id)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    assert!(chain(db, company.tenant_id).await.is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_maintenance_of_a_server_with_a_master_key_seals_the_log() {
    let unkeyed = TestApi::start().await;
    let company = companies::create(
        &unkeyed,
        "chaine-maintenance",
        "jeanne@chaine-maintenance.example",
    )
    .await;
    // A server without a master key has no key to seal with: that is
    // `audit_chain::server_key`'s unit test, on the configuration.
    let keyed = TestApi::start_with(&[("MIMORYA_MASTER_KEY", PUBLIC_TEST_MASTER_KEYS[0])]).await;
    let db = &keyed.app.services.db;
    let mut report = verify(db, company.tenant_id).await;
    for _ in 0..100 {
        if report.unsealed == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        report = verify(db, company.tenant_id).await;
    }
    assert_eq!(report.unsealed, 0, "{report:?}");
    assert!(report.verified > 0 && report.broken == 0, "{report:?}");
}
