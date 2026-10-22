//! Searching memory by meaning (ADR-074): once the company chose a model,
//! its active memories are embedded, and a turn recalls what is close to
//! the request, within what its user may read only. Without a model nothing
//! is sent; when the model fails, the words still find. A proposal is sent
//! once approved. How the indexing holds up: `memory_indexing.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{tenant, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_db::repositories::memory::{NewMemory, create_memory};
use serde_json::json;
use support::TestApi;
use support::embeddings::{
    ASKED, MODEL, VANS, back_to_words, choose, embedding_usage, forget, index, recalled, remember,
    start, vector_of,
};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_turn_recalls_by_meaning_what_its_user_may_read_once_indexed() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let marc = api.sign_in(user::MARC).await;
    let vans = remember(&lucas, VANS.0, VANS.1).await;
    let marc_vans = remember(
        &marc,
        "Fourgons de la direction",
        "Les fourgons de la direction restent au siège.",
    )
    .await;

    // No model chosen: nothing is sent, the words alone find.
    let used = embedding_usage(&api).await.len();
    assert_eq!(index(&api).await, 0);
    assert!(api.embedder.embedded().is_empty());
    assert!(!recalled(&api, &lucas, ASKED).await.contains(VANS.0));
    assert_eq!(embedding_usage(&api).await.len(), used);

    choose(&paul, MODEL, 0.15).await;
    assert!(index(&api).await >= 2);
    assert_eq!(vector_of(&api, vans).await, (Some(MODEL.into()), true));
    let turn = recalled(&api, &lucas, ASKED).await;
    assert!(turn.contains(VANS.0), "{turn}");
    // As close, Marc's personal memory never reaches Lucas.
    assert_eq!(vector_of(&api, marc_vans).await, (Some(MODEL.into()), true));
    assert!(!turn.contains("Fourgons"), "{turn}");
    // The request's vector, recorded as Lucas's, in his run.
    let usage = embedding_usage(&api).await;
    assert_eq!(usage[0]["userId"], json!(user::LUCAS));
    assert!(usage[0]["runId"].is_string(), "{usage:?}");
    assert_eq!(usage[0]["model"], MODEL);

    forget(&lucas, vans).await;
    forget(&marc, marc_vans).await;
    back_to_words(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_words_still_find_while_the_model_fails() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    choose(&paul, MODEL, 0.15).await;
    index(&api).await;

    api.embedder.fail(true);
    let vans = remember(&lucas, VANS.0, VANS.1).await;
    assert_eq!(index(&api).await, 0);
    assert_eq!(vector_of(&api, vans).await, (None, false));
    let shown = paul.get("/admin/embedding").await.json();
    assert_eq!(
        shown["settings"]["indexingError"],
        "Fournisseur indisponible"
    );
    let turn = recalled(&api, &lucas, "Quand ont lieu les retours chantier ?").await;
    assert!(turn.contains("Retours chantier équipe Nord"), "{turn}");
    assert!(!recalled(&api, &lucas, ASKED).await.contains(VANS.0));

    // Back up: indexed at the next pass, the failure forgotten.
    api.embedder.fail(false);
    assert_eq!(index(&api).await, 1);
    let shown = paul.get("/admin/embedding").await.json();
    assert_eq!(shown["settings"]["indexingError"], json!(null));
    assert_eq!(shown["settings"]["embedded"], shown["settings"]["memories"]);
    assert!(recalled(&api, &lucas, ASKED).await.contains(VANS.0));

    forget(&lucas, vans).await;
    back_to_words(&paul).await;
}

/// A personal memory the agent proposed to Lucas.
async fn proposed_to_lucas(api: &TestApi) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id = create_memory(
        &mut tx,
        NewMemory {
            tenant_id: tenant::ID,
            scope: MemoryScope::User,
            owner_user_id: Some(user::LUCAS),
            org_unit_id: None,
            kind: MemoryKind::Preference,
            title: "Badge du local".into(),
            content: "Le badge du local s’obtient auprès du gardien.".into(),
            sensitivity: Sensitivity::Internal,
            status: MemoryStatus::Proposed,
            source: MemorySource::manual(None),
            created_by_user_id: Some(user::LUCAS),
            created_by_run_id: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_is_sent_once_approved() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    choose(&paul, MODEL, 0.15).await;
    index(&api).await;

    // Not indexed along with a memory written meanwhile: not approved yet.
    let proposal = proposed_to_lucas(&api).await;
    let vans = remember(&lucas, VANS.0, VANS.1).await;
    assert_eq!(index(&api).await, 1);
    assert_eq!(vector_of(&api, vans).await, (Some(MODEL.into()), true));
    assert_eq!(vector_of(&api, proposal).await, (None, false));
    assert!(
        !api.embedder
            .embedded()
            .iter()
            .any(|t| t.contains("gardien"))
    );

    let review = format!("/memory/{proposal}/review");
    let approved = lucas.post(&review, json!({ "decision": "approve" })).await;
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.text());
    assert_eq!(index(&api).await, 1);
    assert_eq!(vector_of(&api, proposal).await, (Some(MODEL.into()), true));

    forget(&lucas, proposal).await;
    forget(&lucas, vans).await;
    back_to_words(&paul).await;
}
