//! The audit log chained by company (ADR-178): sealed by the maintenance
//! of a server with a master key, each company in its own chain, verified
//! from its first place. An entry the database's owner changes or removes,
//! a seal rewritten, two places swapped or another key break the chain
//! where they are, and every break is counted. A seal is never changed nor
//! taken away, even by the owner, unless they drop that rule first; seals
//! taken away and set again, or entries removed from the chain's end, are
//! refused by the sealing that remembers the last seal, and shown by an
//! anchor logged outside the database.

mod support;

use std::time::Duration;

use mimorya_api::secrets::{PUBLIC_TEST_MASTER_KEYS, SecretCipher};
use mimorya_api::services::audit_chain::{
    AnchorFound, ChainBreak, ChainKey, ChainMoved, ChainReport, SealedHeads, seal_audit_log,
    seal_company, verify_company,
};
use mimorya_db::Database;
use support::{TestApi, companies, owner_url};
use uuid::Uuid;

fn key(index: usize) -> ChainKey {
    ChainKey::of(&SecretCipher::from_base64(PUBLIC_TEST_MASTER_KEYS[index]).unwrap()).unwrap()
}

async fn append(db: &Database, tenant_id: Uuid, action: &str) -> i64 {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let id = sqlx::query_scalar(
        "insert into audit_log (tenant_id, actor_type, action, outcome, metadata, ip) \
         values ($1, 'user', $2, 'success', '{\"détail\": [1, 2.50, null]}', '192.0.2.7') \
         returning id",
    )
    .bind(tenant_id)
    .bind(action)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// The company's sealed entries, in the chain's order: entry and place.
async fn chain(db: &Database, tenant_id: Uuid) -> Vec<(i64, i64)> {
    let mut tx = db.begin(tenant_id).await.unwrap();
    sqlx::query_as(
        "select id, chain_seq from audit_log where chain_seq is not null order by chain_seq",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// Runs `statement` on the entry `id` as the database's owner, who is not
/// held by the application's rights.
async fn as_owner(statement: &str, id: i64) -> sqlx::Result<()> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let done = sqlx::query(statement).bind(id).execute(&owner).await;
    owner.close().await;
    done.map(|_| ())
}

/// The same, the rule that keeps the seals dropped for the statement, as
/// an owner set on changing them would.
async fn as_owner_over_the_seals(statement: &str, id: i64) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let mut tx = owner.begin().await.unwrap();
    sqlx::query("alter table audit_log disable trigger audit_log_keep_seals")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(statement)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("alter table audit_log enable trigger audit_log_keep_seals")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    owner.close().await;
}

async fn seal(db: &Database, heads: &SealedHeads, tenant_id: Uuid) -> anyhow::Result<usize> {
    seal_company(db, &key(0), heads, tenant_id).await
}

async fn verify(db: &Database, tenant_id: Uuid) -> ChainReport {
    verify_company(db, &key(0), tenant_id, &[]).await.unwrap()
}

fn intact(report: &ChainReport, verified: i64) {
    assert_eq!(report.broken, 0, "{report:?}");
    assert_eq!(report.verified, verified, "{report:?}");
    assert_eq!(
        report.head.as_ref().map(|head| head.chain_seq),
        Some(verified)
    );
}

fn first_break(report: &ChainReport) -> Option<ChainBreak> {
    report.breaks.first().cloned()
}

fn altered((entry_id, seq): (i64, i64)) -> Option<ChainBreak> {
    Some(ChainBreak::Altered { seq, entry_id })
}

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
async fn an_entry_removed_breaks_at_its_place_and_the_end_shows_against_an_anchor() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company = companies::create(&api, "chaine-trouee", "jeanne@chaine-trouee.example").await;
    for action in ["test.a", "test.b", "test.c"] {
        append(db, company.tenant_id, action).await;
    }
    let heads = SealedHeads::default();
    seal(db, &heads, company.tenant_id).await.unwrap();
    let places = chain(db, company.tenant_id).await;
    let count = places.len() as i64;
    let logged = verify(db, company.tenant_id).await.head.unwrap();

    // The chain's end removed: what is left is intact, but the seal logged
    // at the last place is no longer there.
    let last = places[places.len() - 1].0;
    as_owner("delete from audit_log where id = $1", last)
        .await
        .unwrap();
    intact(&verify(db, company.tenant_id).await, count - 1);
    let anchors = [logged.clone()];
    let anchored = verify_company(db, &key(0), company.tenant_id, &anchors)
        .await
        .unwrap();
    assert_eq!(anchored.anchors, [AnchorFound::Missing]);
    assert!(!anchored.holds());
    // The sealing that set it refuses to go on from a shorter chain.
    append(db, company.tenant_id, "test.after").await;
    let refused = seal(db, &heads, company.tenant_id).await.unwrap_err();
    assert!(refused.is::<ChainMoved>(), "{refused:#}");
    // A sealing that does not remember it seals the new entry at the
    // removed one's place: the chain then holds another seal there.
    seal(db, &SealedHeads::default(), company.tenant_id)
        .await
        .unwrap();
    intact(&verify(db, company.tenant_id).await, count);
    let anchored = verify_company(db, &key(0), company.tenant_id, &anchors)
        .await
        .unwrap();
    assert_eq!(anchored.anchors, [AnchorFound::Differs]);

    // An entry in the middle removed: its place is missing, the next entry
    // cannot be checked, the others still are.
    let (removed, seq) = places[1];
    as_owner("delete from audit_log where id = $1", removed)
        .await
        .unwrap();
    let report = verify(db, company.tenant_id).await;
    assert_eq!(report.breaks, [ChainBreak::Missing { seq }]);
    assert_eq!((report.broken, report.verified), (1, count - 2));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn seals_taken_away_then_set_again_show_against_the_seals_remembered_and_logged() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company =
        companies::create(&api, "chaine-rescellee", "jeanne@chaine-rescellee.example").await;
    for action in ["test.a", "test.b", "test.c"] {
        append(db, company.tenant_id, action).await;
    }
    let heads = SealedHeads::default();
    seal(db, &heads, company.tenant_id).await.unwrap();
    let places = chain(db, company.tenant_id).await;
    let logged = verify(db, company.tenant_id).await.head.unwrap();
    // A process started since remembers the chain as it found it.
    let restarted = SealedHeads::default();
    restarted.load(db).await.unwrap();

    // The owner takes every seal away, past the rule that keeps them, and
    // changes an entry.
    as_owner_over_the_seals(
        "update audit_log set chain_seq = null, chain_hash = null \
         where tenant_id = (select tenant_id from audit_log where id = $1)",
        places[0].0,
    )
    .await;
    let change = "update audit_log set action = 'test.falsifiée' where id = $1";
    as_owner(change, places[1].0).await.unwrap();

    // The sealing that set or found the seals refuses to seal again.
    for remembered in [&heads, &restarted] {
        let refused = seal(db, remembered, company.tenant_id).await.unwrap_err();
        assert!(refused.is::<ChainMoved>(), "{refused:#}");
        assert_eq!(chain(db, company.tenant_id).await, []);
    }
    // One that remembers nothing seals again: the database alone shows an
    // intact chain, the seal logged before does not hold.
    seal(db, &SealedHeads::default(), company.tenant_id)
        .await
        .unwrap();
    intact(&verify(db, company.tenant_id).await, places.len() as i64);
    let anchored = verify_company(db, &key(0), company.tenant_id, &[logged])
        .await
        .unwrap();
    assert_eq!(anchored.anchors, [AnchorFound::Differs]);
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
