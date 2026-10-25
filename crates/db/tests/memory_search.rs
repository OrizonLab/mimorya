//! Searching memory by meaning (ADR-074): only within what the subject may
//! read, only the chosen model's vectors, only those close enough; fused
//! with the words' ranking. And the queue of memories awaiting a vector, and
//! the companies whose memories or documents' passages await theirs.
//! A test that stores vectors only to search leaves its transaction
//! uncommitted: it is rolled back.

mod support;

use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::demo::{self, tenant, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemoryStatus, memory_read_filter};
use mimorya_db::repositories::document_embeddings::{
    clear_passage_embeddings, passages_to_embed, store_passage_embedding,
};
use mimorya_db::repositories::memory::{
    Meaning, MemorySource, NewMemory, create_memory, search_memory,
};
use mimorya_db::repositories::memory_embeddings::{
    IndexProgress, clear_embeddings, index_progress, memories_to_embed, store_embedding,
    tenants_awaiting_embeddings,
};
use mimorya_db::testing::OTHER_TENANT_ID;
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

const MODEL: &str = "bge-m3";

/// Adds the other team's procedure once, right after the reset.
fn add_team_south_procedure(urls: &mimorya_db::testing::TestDatabaseUrls) {
    let app = urls.app.clone();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            let db = support::connect(&app).await;
            let mut tx = db.begin(tenant::ID).await.unwrap();
            create_memory(
                &mut tx,
                NewMemory {
                    tenant_id: tenant::ID,
                    scope: MemoryScope::OrgUnit,
                    owner_user_id: None,
                    org_unit_id: Some(unit::TEAM_SOUTH),
                    kind: MemoryKind::Procedure,
                    title: "Retours chantier équipe Sud".into(),
                    content: "Les retours chantier de l’équipe Sud ont lieu le mardi matin.".into(),
                    sensitivity: Sensitivity::Internal,
                    status: MemoryStatus::Active,
                    source: MemorySource::manual(None),
                    created_by_user_id: None,
                    created_by_run_id: None,
                },
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        });
}

async fn app() -> Database {
    support::connect(&support::urls(add_team_south_procedure).app).await
}

/// The company chooses `MODEL`.
async fn choose_model(db: &Database, tenant_id: Uuid) {
    let mut tx = db.begin(tenant_id).await.unwrap();
    sqlx::query(
        "insert into tenant_embedding_settings (tenant_id, provider, model, base_url, \
         min_similarity) values ($1, 'ollama', $2, 'http://ollama.test:11434/v1', 0.5)",
    )
    .bind(tenant_id)
    .bind(MODEL)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Suspends or reactivates a company, as the operator does (owner role).
async fn set_status(tenant_id: Uuid, status: &str) {
    let owner = support::connect(&support::urls(add_team_south_procedure).owner).await;
    sqlx::query("update tenants set status = $2 where id = $1")
        .bind(tenant_id)
        .bind(status)
        .execute(owner.pool())
        .await
        .unwrap();
}

/// A unit vector along `axis`.
fn axis(axis: usize) -> Vec<f32> {
    let mut vector = vec![0.0; 1024];
    vector[axis] = 1.0;
    vector
}

async fn id_of(tx: &mut TenantTx, title: &str) -> Uuid {
    sqlx::query_scalar("select id from memory_items where title = $1")
        .bind(title)
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

/// Gives the memory of `title` a vector of `model`, after wiping them all.
async fn embed(tx: &mut TenantTx, vectors: &[(&str, Vec<f32>, &str)]) {
    clear_embeddings(tx).await.unwrap();
    for (title, vector, model) in vectors {
        let id = id_of(tx, title).await;
        store_embedding(tx, id, Some(vector), model).await.unwrap();
    }
}

/// What Lucas finds for `query`, of meaning along `axis`.
async fn lucas_finds(tx: &mut TenantTx, query: &str, along: usize) -> Vec<String> {
    let engine = demo::authorization_engine();
    let subject = demo::subject(user::LUCAS);
    let filter = memory_read_filter(&engine, &subject, &Actor::user());
    let vector = axis(along);
    let meaning = Meaning {
        vector: &vector,
        model: MODEL,
        min_similarity: 0.5,
    };
    let hits = search_memory(tx, &filter, query, Some(meaning), 5, subject.clearance())
        .await
        .unwrap();
    hits.into_iter().map(|h| h.title).collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn finds_by_meaning_what_the_subject_may_read_and_is_close_enough() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    embed(
        &mut tx,
        &[
            ("Horaires d’intervention", axis(0), MODEL),
            // Out of Lucas's reach.
            ("Retours chantier équipe Sud", axis(0), MODEL),
            ("Format des synthèses", axis(0), MODEL),
            // Another model's vector does not compare.
            ("Prévenance des déplacements", axis(0), "mistral-embed"),
            // Too far.
            ("Clôture d’une intervention", axis(1), MODEL),
        ],
    )
    .await;

    // No word in common: found by meaning only.
    let found = lucas_finds(&mut tx, "Quelle tenue porter ?", 0).await;
    assert_eq!(found, ["Horaires d’intervention"]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_both_words_and_meaning_find_ranks_first() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    embed(
        &mut tx,
        &[
            ("Horaires d’intervention", axis(0), MODEL),
            ("Retours chantier équipe Nord", axis(0), MODEL),
        ],
    )
    .await;
    let found = lucas_finds(&mut tx, "retours chantier", 0).await;
    assert_eq!(
        found,
        ["Retours chantier équipe Nord", "Horaires d’intervention"]
    );
    // Without a vector, the words alone.
    let found = lucas_finds(&mut tx, "retours chantier", 2).await;
    assert_eq!(found, ["Retours chantier équipe Nord"]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn memories_await_the_chosen_model_s_vector_until_given_or_refused() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    clear_embeddings(&mut tx).await.unwrap();
    let waiting = memories_to_embed(&mut tx, MODEL, 100).await.unwrap();
    assert_eq!(waiting.len(), 6);
    let first = waiting[0].id;
    let second = waiting[1].id;
    store_embedding(&mut tx, first, Some(&axis(0)), MODEL)
        .await
        .unwrap();
    // Refused by the model: not asked again, found by its words only.
    store_embedding(&mut tx, second, None, MODEL).await.unwrap();
    let waiting = memories_to_embed(&mut tx, MODEL, 100).await.unwrap();
    assert_eq!(waiting.len(), 4);
    assert!(waiting.iter().all(|m| m.id != first && m.id != second));
    let progress = index_progress(&mut tx, MODEL).await.unwrap();
    assert_eq!(
        progress,
        IndexProgress {
            embedded: 1,
            total: 6
        }
    );
    // Another model's vectors are awaited again.
    assert_eq!(
        memories_to_embed(&mut tx, "other", 100)
            .await
            .unwrap()
            .len(),
        6
    );
    assert_eq!(clear_embeddings(&mut tx).await.unwrap(), 2);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_an_active_company_that_chose_a_model_awaits_vectors() {
    let db = app().await;
    let awaiting = || async { tenants_awaiting_embeddings(db.pool()).await.unwrap() };
    assert!(awaiting().await.is_empty());
    choose_model(&db, tenant::ID).await;
    assert_eq!(awaiting().await, [tenant::ID]);
    // The other company, its own memory awaiting, once it chose a model too,
    // and not while suspended.
    choose_model(&db, OTHER_TENANT_ID).await;
    let mut both = awaiting().await;
    both.sort();
    let mut expected = vec![tenant::ID, OTHER_TENANT_ID];
    expected.sort();
    assert_eq!(both, expected);
    set_status(OTHER_TENANT_ID, "suspended").await;
    assert_eq!(awaiting().await, [tenant::ID]);
    set_status(OTHER_TENANT_ID, "active").await;
    let mut tx = db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query("delete from tenant_embedding_settings")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut tx = db.begin(tenant::ID).await.unwrap();
    for memory in memories_to_embed(&mut tx, MODEL, 100).await.unwrap() {
        store_embedding(&mut tx, memory.id, Some(&axis(0)), MODEL)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    // Its documents' passages await theirs (ADR-096).
    assert_eq!(awaiting().await, [tenant::ID]);
    let mut tx = db.begin(tenant::ID).await.unwrap();
    for passage in passages_to_embed(&mut tx, MODEL, 100).await.unwrap() {
        store_passage_embedding(&mut tx, passage.id, Some(&axis(0)), MODEL)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    assert!(
        tenants_awaiting_embeddings(db.pool())
            .await
            .unwrap()
            .is_empty()
    );

    let mut tx = db.begin(tenant::ID).await.unwrap();
    clear_embeddings(&mut tx).await.unwrap();
    clear_passage_embeddings(&mut tx).await.unwrap();
    sqlx::query("delete from tenant_embedding_settings")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}
