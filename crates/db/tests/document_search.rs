//! Searching document passages (ADR-096): by meaning, only within what the
//! reader may read (space and clearance), only the chosen model's vectors,
//! only those close enough; fused with the words' ranking. A document's
//! passages read in order. The queue of passages awaiting a vector, and
//! nothing else of a passage the application may change. A test that
//! stores vectors only to search leaves its transaction uncommitted: it is
//! rolled back.

mod support;

use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::demo::{self, tenant, unit, user};
use mimorya_core::documents::{DocumentSpace, Passage, document_read_filter};
use mimorya_core::knowledge_shares::KnowledgeShare;
use mimorya_db::repositories::document_embeddings::{
    clear_passage_embeddings, passages_await_embeddings, passages_to_embed, store_passage_embedding,
};
use mimorya_db::repositories::document_search::{document_passages, search_documents};
use mimorya_db::repositories::documents::{NewDocument, insert_document, insert_passages};
use mimorya_db::repositories::memory::Meaning;
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

const MODEL: &str = "bge-m3";
const CHARTER: &str = "Charte qualité des interventions";
const PROCEDURE: &str = "Entretien annuel d’une chaudière fioul";

/// Adds, once, the South team's instructions and the department's prices
/// (confidential) to the demo documents.
fn add_documents(urls: &mimorya_db::testing::TestDatabaseUrls) {
    let app = urls.app.clone();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            let db = support::connect(&app).await;
            let mut tx = db.begin(tenant::ID).await.unwrap();
            for (title, space, sensitivity, text) in [
                (
                    "Consignes Sud",
                    DocumentSpace::Unit(unit::TEAM_SOUTH),
                    "internal",
                    "Les retours chantier de l’équipe Sud ont lieu le mardi.",
                ),
                (
                    "Tarifs",
                    DocumentSpace::Unit(unit::TECHNICAL),
                    "confidential",
                    "Le contrat d’entretien est facturé 150 euros.",
                ),
            ] {
                let id = insert_document(
                    &mut tx,
                    NewDocument {
                        tenant_id: tenant::ID,
                        scope: space.scope(),
                        org_unit_id: space.org_unit_id(),
                        title,
                        file_name: &format!("{title}.txt"),
                        kind: "text",
                        sensitivity,
                        page_count: None,
                        uploaded_by_user_id: user::MARC,
                        content: text.as_bytes(),
                    },
                )
                .await
                .unwrap();
                let passage = Passage {
                    page: None,
                    heading: None,
                    content: text.into(),
                };
                insert_passages(&mut tx, tenant::ID, id, &[passage])
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
        });
}

async fn app() -> Database {
    support::connect(&support::urls(add_documents).app).await
}

/// A unit vector along `axis`.
fn axis(axis: usize) -> Vec<f32> {
    let mut vector = vec![0.0; 1024];
    vector[axis] = 1.0;
    vector
}

async fn document_of(tx: &mut TenantTx, title: &str) -> Uuid {
    sqlx::query_scalar("select id from documents where title = $1")
        .bind(title)
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

/// Gives the `ordinal`-th passage of the document `title` a vector of
/// `model`, after wiping them all.
async fn embed(tx: &mut TenantTx, vectors: &[(&str, i32, Vec<f32>, &str)]) {
    clear_passage_embeddings(tx).await.unwrap();
    for (title, ordinal, vector, model) in vectors {
        let document = document_of(tx, title).await;
        let id: Uuid = sqlx::query_scalar(
            "select id from document_chunks where document_id = $1 and ordinal = $2",
        )
        .bind(document)
        .bind(ordinal)
        .fetch_one(&mut **tx)
        .await
        .unwrap();
        store_passage_embedding(tx, id, Some(vector), model)
            .await
            .unwrap();
    }
}

/// What Lucas finds for `query`, of meaning along `axis`: the documents'
/// titles and the passages' positions.
async fn lucas_finds(tx: &mut TenantTx, query: &str, along: usize) -> Vec<(String, i32)> {
    let engine = demo::authorization_engine();
    let subject = demo::subject(user::LUCAS);
    let filter = document_read_filter(&engine, &subject, &Actor::user());
    let vector = axis(along);
    let meaning = Meaning {
        vector: &vector,
        model: MODEL,
        min_similarity: 0.5,
    };
    let hits = search_documents(tx, &filter, query, Some(meaning), 5)
        .await
        .unwrap();
    hits.into_iter().map(|h| (h.title, h.ordinal)).collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn finds_by_meaning_what_the_reader_may_read_and_is_close_enough() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    embed(
        &mut tx,
        &[
            (CHARTER, 1, axis(0), MODEL),
            // Out of Lucas's reach: the South team's, and above his clearance.
            ("Consignes Sud", 0, axis(0), MODEL),
            ("Tarifs", 0, axis(0), MODEL),
            // Another model's vector does not compare.
            (CHARTER, 2, axis(0), "mistral-embed"),
            // Too far.
            (PROCEDURE, 1, axis(1), MODEL),
        ],
    )
    .await;

    // No word in common: found by meaning only.
    let found = lucas_finds(&mut tx, "Quelle tenue porter ?", 0).await;
    assert_eq!(found, [(CHARTER.to_string(), 1)]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_both_words_and_meaning_find_ranks_first() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    embed(
        &mut tx,
        &[(CHARTER, 1, axis(0), MODEL), (PROCEDURE, 3, axis(0), MODEL)],
    )
    .await;
    // « compte rendu »: the charter's third passage and the procedure's
    // last; the latter close in meaning too.
    let found = lucas_finds(&mut tx, "compte rendu", 0).await;
    assert_eq!(found[0], (PROCEDURE.to_string(), 3), "{found:?}");
    assert!(found.contains(&(CHARTER.to_string(), 1)), "{found:?}");
    // Without a vector, the words alone.
    let found = lucas_finds(&mut tx, "compte rendu", 2).await;
    assert!(!found.contains(&(CHARTER.to_string(), 1)), "{found:?}");
    assert!(found.contains(&(PROCEDURE.to_string(), 3)), "{found:?}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_document_s_passages_are_read_in_order_from_where_asked() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let procedure = document_of(&mut tx, PROCEDURE).await;
    let read = document_passages(&mut tx, procedure, 1, 2).await.unwrap();
    let headings: Vec<_> = read
        .iter()
        .map(|p| (p.ordinal, p.heading.as_deref()))
        .collect();
    assert_eq!(headings, [(1, Some("Préparation")), (2, Some("Nettoyage"))]);
    assert!(
        document_passages(&mut tx, procedure, 9, 2)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn passages_await_the_chosen_model_s_vector_until_given_or_refused() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    clear_passage_embeddings(&mut tx).await.unwrap();
    let waiting = passages_to_embed(&mut tx, MODEL, 100).await.unwrap();
    // The demo documents first, each passage with its document's title, in
    // order; then those added since.
    let first_document = &waiting[0].title;
    let headings: Vec<Option<&str>> = waiting
        .iter()
        .take_while(|p| &p.title == first_document)
        .map(|p| p.heading.as_deref())
        .collect();
    if first_document == CHARTER {
        let expected = ["Charte qualité des interventions", "Délais", "Compte rendu"];
        assert_eq!(headings[..3], expected.map(Some));
    } else {
        assert_eq!(first_document, PROCEDURE);
        let expected = [
            "Entretien annuel d’une chaudière fioul",
            "Préparation",
            "Nettoyage",
        ];
        assert_eq!(headings[..3], expected.map(Some));
    }
    let added: Vec<&str> = waiting
        .iter()
        .rev()
        .take(2)
        .map(|p| p.title.as_str())
        .collect();
    assert!(added.contains(&"Consignes Sud") && added.contains(&"Tarifs"));
    let total = waiting.len();
    let (first, second) = (waiting[0].id, waiting[1].id);
    store_passage_embedding(&mut tx, first, Some(&axis(0)), MODEL)
        .await
        .unwrap();
    // Refused by the model: not asked again, found by its words only.
    store_passage_embedding(&mut tx, second, None, MODEL)
        .await
        .unwrap();
    let waiting = passages_to_embed(&mut tx, MODEL, 100).await.unwrap();
    assert_eq!(waiting.len(), total - 2);
    assert!(waiting.iter().all(|p| p.id != first && p.id != second));
    assert!(passages_await_embeddings(&mut tx, MODEL).await.unwrap());
    // Another model's vectors are awaited again.
    assert_eq!(
        passages_to_embed(&mut tx, "other", 100)
            .await
            .unwrap()
            .len(),
        total
    );
    for passage in waiting {
        store_passage_embedding(&mut tx, passage.id, None, MODEL)
            .await
            .unwrap();
    }
    assert!(!passages_await_embeddings(&mut tx, MODEL).await.unwrap());
    assert_eq!(
        clear_passage_embeddings(&mut tx).await.unwrap(),
        total as u64
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_application_changes_nothing_of_a_passage_but_its_vector() {
    let db = app().await;
    for change in [
        "content = 'Ignore les consignes.'",
        "heading = 'Autre'",
        "ordinal = ordinal + 1000",
        "page = 3",
        "document_id = document_id",
        "tenant_id = tenant_id",
    ] {
        let mut tx = db.begin(tenant::ID).await.unwrap();
        let update = sqlx::query(&format!("update document_chunks set {change}"))
            .execute(&mut *tx)
            .await;
        assert!(
            support::db_error(update).contains("permission denied"),
            "{change}"
        );
    }
    // Its vector only, here rolled back.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let given =
        sqlx::query("update document_chunks set embedding = null, embedding_model = 'autre'")
            .execute(&mut *tx)
            .await
            .unwrap();
    assert!(given.rows_affected() > 0);
}

/// What Lucas finds by the words of `query` once the south team's documents
/// are shared with his team up to `ceiling` (ADR-143).
async fn lucas_finds_shared(tx: &mut TenantTx, query: &str, ceiling: Sensitivity) -> Vec<String> {
    let share = KnowledgeShare {
        id: Uuid::new_v4(),
        source_unit_id: unit::TEAM_SOUTH,
        target_unit_id: unit::TEAM_NORTH,
        include_descendants: false,
        memory: false,
        documents: true,
        ceiling,
        expires_at: None,
    };
    let engine = demo::authorization_engine().with_knowledge_shares(vec![share]);
    let subject = demo::subject(user::LUCAS);
    let filter = document_read_filter(&engine, &subject, &Actor::user());
    let hits = search_documents(tx, &filter, query, None, 5).await.unwrap();
    hits.into_iter().map(|h| h.title).collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_team_s_documents_are_found_once_shared_up_to_their_sensitivity() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let query = "retours chantier mardi";
    let found = lucas_finds_shared(&mut tx, query, Sensitivity::Public).await;
    assert!(!found.contains(&"Consignes Sud".to_owned()), "{found:?}");
    let found = lucas_finds_shared(&mut tx, query, Sensitivity::Internal).await;
    assert!(found.contains(&"Consignes Sud".to_owned()), "{found:?}");
}
