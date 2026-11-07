//! What the database alone cannot show of an audit chain (ADR-178), and
//! the guards kept outside it: entries removed from the chain's end, and
//! seals taken away then set again on changed entries, give a chain that
//! verifies; the sealing that set or found the last seal refuses to go on,
//! and the anchor it logged shows the change. A process reads the chains at
//! its start until it has read them all, never one it remembers again. A
//! sound chain is never refused: sealed in turn by several processes, by a
//! process started since, or a company's first seals.

mod support;

use mimorya_api::services::audit_chain::{
    AnchorFound, ChainMoved, SealedHeads, anchor_text, verify_company,
};
use support::audit_chain::{
    append, as_owner, as_owner_over_the_seals, chain, intact, key, seal, verify,
};
use support::{TestApi, companies, owner_url};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_chain_s_end_removed_is_refused_and_shown_against_an_anchor() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company =
        companies::create(&api, "chaine-ecourtee", "jeanne@chaine-ecourtee.example").await;
    for action in ["test.a", "test.b", "test.c"] {
        append(db, company.tenant_id, action).await;
    }
    let heads = SealedHeads::default();
    seal(db, &heads, company.tenant_id).await.unwrap();
    let places = chain(db, company.tenant_id).await;
    let count = places.len() as i64;
    let logged = verify(db, company.tenant_id).await.head.unwrap();
    // The anchor as the sealing logs it names the company and its place.
    let anchor = anchor_text(company.tenant_id, &logged);
    assert!(
        anchor.starts_with(&format!("{}:{count}:", company.tenant_id)),
        "{anchor}"
    );

    // The chain's end removed: what is left is intact, but the seal logged
    // at the last place is no longer there.
    let last = places[places.len() - 1].0;
    as_owner("delete from audit_log where id = $1", last)
        .await
        .unwrap();
    intact(&verify(db, company.tenant_id).await, count - 1);
    let anchors = [logged];
    let anchored = verify_company(db, &key(0), company.tenant_id, &anchors)
        .await
        .unwrap();
    assert_eq!(anchored.anchors, [AnchorFound::Missing]);
    assert!(!anchored.holds());
    // The sealing that set it refuses to go on from a shorter chain, even
    // after its process read the chains again: a company it remembers is
    // never read again.
    heads.load(db).await.unwrap();
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
async fn a_sound_chain_is_never_refused() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company = companies::create(&api, "chaine-saine", "jeanne@chaine-saine.example").await;
    // Two processes seal in turn, each after the other's last seal.
    let (first, second) = (SealedHeads::default(), SealedHeads::default());
    for (round, heads) in [&first, &second, &first, &second, &first]
        .into_iter()
        .enumerate()
    {
        append(db, company.tenant_id, &format!("test.round-{round}")).await;
        let sealed = seal(db, heads, company.tenant_id).await;
        assert!(
            matches!(sealed, Ok(n) if n > 0),
            "round {round}: {sealed:?}"
        );
    }
    // A process started since finds the chain as it is, and goes on.
    let restarted = SealedHeads::default();
    restarted.load(db).await.unwrap();
    append(db, company.tenant_id, "test.restarted").await;
    assert_eq!(seal(db, &restarted, company.tenant_id).await.unwrap(), 1);
    // And so does one that sealed before it.
    append(db, company.tenant_id, "test.again").await;
    assert_eq!(seal(db, &second, company.tenant_id).await.unwrap(), 1);
    // A company whose first entries come after a process started is sealed
    // by it from the first place.
    let later = companies::create(&api, "chaine-nouvelle", "jeanne@chaine-nouvelle.example").await;
    append(db, later.tenant_id, "test.first").await;
    assert!(seal(db, &restarted, later.tenant_id).await.unwrap() > 0);
    assert_eq!(chain(db, later.tenant_id).await[0].1, 1);
    for tenant_id in [company.tenant_id, later.tenant_id] {
        let places = chain(db, tenant_id).await.len() as i64;
        intact(&verify(db, tenant_id).await, places);
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn chains_not_read_at_startup_are_read_at_the_next_try_and_then_kept() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let company = companies::create(&api, "chaine-relue", "jeanne@chaine-relue.example").await;
    append(db, company.tenant_id, "test.read").await;
    seal(db, &SealedHeads::default(), company.tenant_id)
        .await
        .unwrap();

    // The chains cannot be read: the process remembers none of them, and
    // says so, for the maintenance to try again.
    let restarted = SealedHeads::default();
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("revoke select on audit_log from mimorya_app")
        .execute(&owner)
        .await
        .unwrap();
    let unread = restarted.load(db).await;
    sqlx::query("grant select on audit_log to mimorya_app")
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    assert!(unread.is_err(), "{unread:?}");

    // Tried again, it remembers each chain as it finds it.
    restarted.load(db).await.unwrap();
    let places = chain(db, company.tenant_id).await;
    let last = places[places.len() - 1].0;
    as_owner("delete from audit_log where id = $1", last)
        .await
        .unwrap();
    // Read once more, the chain changed meanwhile is not taken for its own.
    restarted.load(db).await.unwrap();
    append(db, company.tenant_id, "test.after").await;
    let refused = seal(db, &restarted, company.tenant_id).await.unwrap_err();
    assert!(refused.is::<ChainMoved>(), "{refused:#}");
}
