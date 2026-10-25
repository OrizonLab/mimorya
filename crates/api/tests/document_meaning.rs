//! Searching documents by meaning (ADR-096): once the company chose an
//! embedding model, the passages of its documents get their vectors, with
//! its memories, and `document_search` finds what is close to a request
//! with no word in common, within what its user may read only. A deposit
//! wakes the indexer; going back to words wipes every passage's vector.

mod support;

use std::time::Duration;

use mimorya_core::demo::{tenant, unit, user};
use support::TestApi;
use support::document_search::found;
use support::documents::{Target, deposited, remove};
use support::embeddings::{ASKED, MODEL, back_to_words, choose, indexed, start, start_indexing};
use uuid::Uuid;

/// The model of each passage's vector of the document `id` (`None`: not
/// asked yet), and whether it has one.
async fn vectors_of(api: &TestApi, id: Uuid) -> Vec<(Option<String>, bool)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select embedding_model, embedding is not null from document_chunks \
         where document_id = $1 order by ordinal",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// How many passages of the company have a vector, or were refused one.
async fn passages_given(api: &TestApi) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar("select count(*) from document_chunks where embedding_model is not null")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn passages_are_found_by_meaning_within_reach_once_indexed() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let julien = api.sign_in(user::JULIEN).await;
    let nadia = api.sign_in(user::NADIA).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let vans = deposited(
        &julien,
        "Tournées Nord.md",
        Target::unit(unit::TEAM_NORTH),
        "Les camionnettes partent du dépôt de Lyon à 7 h.".as_bytes(),
    )
    .await;
    let south = deposited(
        &nadia,
        "Tournées Sud.md",
        Target::unit(unit::TEAM_SOUTH),
        "Les fourgons partent de Marseille à 8 h.".as_bytes(),
    )
    .await;

    // No model chosen: no word in common, nothing found, nothing sent.
    assert!(
        !found(&api, &lucas, ASKED)
            .await
            .iter()
            .any(|(id, _)| *id == vans)
    );
    assert!(api.embedder.embedded().is_empty());

    choose(&paul, MODEL, 0.15).await;
    let pass = indexed(&api).await;
    assert!(pass.passages >= 2, "{pass:?}");
    assert_eq!(vectors_of(&api, vans).await, [(Some(MODEL.into()), true)]);
    // The passage is embedded with its document's title.
    assert!(
        api.embedder
            .embedded()
            .contains(&"Tournées Nord\n\nLes camionnettes partent du dépôt de Lyon à 7 h.".into())
    );
    let hits = found(&api, &lucas, ASKED).await;
    assert!(hits.iter().any(|(id, _)| *id == vans), "{hits:?}");
    // As close, the South team's never reaches Lucas; Nadia reads it.
    assert!(!hits.iter().any(|(id, _)| *id == south), "{hits:?}");
    let hers = found(&api, &nadia, ASKED).await;
    assert!(hers.iter().any(|(id, _)| *id == south), "{hers:?}");
    assert!(!hers.iter().any(|(id, _)| *id == vans), "{hers:?}");

    // Back to words: every passage's vector is wiped.
    back_to_words(&paul).await;
    assert_eq!(passages_given(&api).await, 0);
    remove(&julien, vans).await;
    remove(&nadia, south).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deposit_wakes_the_indexer() {
    let api = start_indexing().await;
    let paul = api.sign_in(user::PAUL).await;
    let julien = api.sign_in(user::JULIEN).await;
    choose(&paul, MODEL, 0.15).await;
    // The pass the choice wakes gives the demo documents their vectors.
    let mut demo = passages_given(&api).await;
    for _ in 0..200 {
        if demo > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        demo = passages_given(&api).await;
    }
    assert!(demo > 0);

    // Long before the next pass of the minute.
    let vans = deposited(
        &julien,
        "Tournées Nord.md",
        Target::unit(unit::TEAM_NORTH),
        "Les camionnettes partent du dépôt de Lyon à 7 h.".as_bytes(),
    )
    .await;
    let mut given = vectors_of(&api, vans).await;
    for _ in 0..200 {
        if given.iter().all(|(_, embedded)| *embedded) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        given = vectors_of(&api, vans).await;
    }
    assert_eq!(given, [(Some(MODEL.into()), true)]);
    remove(&julien, vans).await;
    back_to_words(&paul).await;
}
