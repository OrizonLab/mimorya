//! The audit log chained by company (ADR-178): sealed by the maintenance
//! of a server with a master key, each company in its own chain, verified
//! from its first place. An entry the database's owner changes or removes,
//! a seal rewritten, two places swapped or another key break the chain
//! where they are; entries removed from its end show only against the last
//! place logged.

mod support;

use std::time::Duration;

use mimorya_api::secrets::{PUBLIC_TEST_MASTER_KEYS, SecretCipher};
use mimorya_api::services::audit_chain::{
    ChainBreak, ChainKey, ChainReport, seal_audit_log, seal_company, verify_company,
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
async fn as_owner(statement: &str, id: i64) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(statement)
        .bind(id)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

async fn verify(db: &Database, tenant_id: Uuid) -> ChainReport {
    verify_company(db, &key(0), tenant_id).await.unwrap()
}

fn intact(report: &ChainReport, verified: i64) {
    assert_eq!(report.broken, None, "{report:?}");
    assert_eq!(report.verified, verified, "{report:?}");
    assert_eq!(
        report.head.as_ref().map(|head| head.chain_seq),
        Some(verified)
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_is_sealed_in_its_own_chain_and_verified_intact() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let acme = companies::create(&api, "chaine-acme", "jeanne@chaine-acme.example").await;
    let other = companies::create(&api, "chaine-autre", "jeanne@chaine-autre.example").await;
    for action in ["test.one", "test.two", "test.three"] {
        append(db, acme.tenant_id, action).await;
    }
    // Not before it is sealed.
    let before = verify(db, acme.tenant_id).await;
    assert_eq!((before.verified, before.broken.clone()), (0, None));
    assert!(before.unsealed >= 3);

    assert!(seal_audit_log(db, &key(0)).await.unwrap() >= 3);
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
    assert_eq!(seal_company(db, &key(0), acme.tenant_id).await.unwrap(), 2);
    append(db, acme.tenant_id, "test.six").await;
    let report = verify(db, acme.tenant_id).await;
    intact(&report, acme_chain.len() as i64 + 2);
    assert_eq!(report.unsealed, 1);

    // Under another key, the first seal already differs.
    let other_key = verify_company(db, &key(1), acme.tenant_id).await.unwrap();
    let first = acme_chain[0].0;
    assert_eq!(
        other_key.broken,
        Some(ChainBreak::Altered {
            seq: 1,
            entry_id: first
        })
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_the_owner_changes_or_removes_breaks_the_chain_where_it_is() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company =
        companies::create(&api, "chaine-falsifiee", "jeanne@chaine-falsifiee.example").await;
    for action in ["test.a", "test.b", "test.c", "test.d"] {
        append(db, company.tenant_id, action).await;
    }
    seal_company(db, &key(0), company.tenant_id).await.unwrap();
    let places = chain(db, company.tenant_id).await;
    let count = places.len() as i64;
    let (third, fourth, last) = (places[2], places[3], places[places.len() - 1]);
    intact(&verify(db, company.tenant_id).await, count);
    let altered = |(entry_id, seq): (i64, i64)| Some(ChainBreak::Altered { seq, entry_id });

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
        as_owner(change, last.0).await;
        assert_eq!(
            verify(db, company.tenant_id).await.broken,
            altered(last),
            "{change}"
        );
        as_owner(undo, last.0).await;
        intact(&verify(db, company.tenant_id).await, count);
    }

    // Two entries swapped places.
    as_owner(
        "update audit_log set chain_seq = 1000000 where id = $1",
        third.0,
    )
    .await;
    let to_third = format!("update audit_log set chain_seq = {} where id = $1", third.1);
    as_owner(&to_third, fourth.0).await;
    let to_fourth = format!(
        "update audit_log set chain_seq = {} where id = $1",
        fourth.1
    );
    as_owner(&to_fourth, third.0).await;
    assert_eq!(
        verify(db, company.tenant_id).await.broken,
        altered((fourth.0, third.1))
    );
    as_owner(
        "update audit_log set chain_seq = 1000000 where id = $1",
        third.0,
    )
    .await;
    as_owner(&to_fourth, fourth.0).await;
    as_owner(&to_third, third.0).await;
    intact(&verify(db, company.tenant_id).await, count);

    // A seal rewritten without the key.
    as_owner(
        "update audit_log set chain_hash = sha256(chain_hash) where id = $1",
        third.0,
    )
    .await;
    assert_eq!(verify(db, company.tenant_id).await.broken, altered(third));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_entry_removed_leaves_its_place_missing_and_the_end_shows_only_against_the_log() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company = companies::create(&api, "chaine-trouee", "jeanne@chaine-trouee.example").await;
    for action in ["test.a", "test.b", "test.c"] {
        append(db, company.tenant_id, action).await;
    }
    seal_company(db, &key(0), company.tenant_id).await.unwrap();
    let places = chain(db, company.tenant_id).await;
    let count = places.len() as i64;

    // The chain's end removed: what is left is intact, its last place
    // lower than the one the sealing logged.
    as_owner(
        "delete from audit_log where id = $1",
        places[places.len() - 1].0,
    )
    .await;
    intact(&verify(db, company.tenant_id).await, count - 1);

    // An entry in the middle removed: the chain breaks at its place.
    let (removed, seq) = places[1];
    as_owner("delete from audit_log where id = $1", removed).await;
    let report = verify(db, company.tenant_id).await;
    assert_eq!(report.broken, Some(ChainBreak::Missing { seq }));
    assert_eq!(report.verified, seq - 1);
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
    // Without a master key, nothing is sealed.
    let db = &unkeyed.app.services.db;
    assert_eq!(verify(db, company.tenant_id).await.verified, 0);

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
    assert!(report.verified > 0 && report.broken.is_none(), "{report:?}");
}
