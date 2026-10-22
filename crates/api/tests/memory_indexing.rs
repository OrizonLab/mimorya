//! How indexing memories by meaning holds up (ADR-074): a text the model
//! refuses (invalid, too long) is not asked again, even a whole batch too
//! long, a batch refused whole as invalid is the model's failure, another model indexes anew, going back to words
//! wipes the vectors, even mid-pass, a pass sends nothing once the choice
//! changed, and a memory written is indexed in the background.

mod support;

use std::time::Duration;

use mimorya_agent::llm::LlmErrorKind;
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{tenant, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_db::repositories::memory::{NewMemory, create_memory};
use serde_json::json;
use support::TestApi;
use support::embeddings::{
    MODEL, VANS, back_to_words, choose, forget, index, remember, start, start_indexing, vector_of,
};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_refused_text_is_not_asked_again_and_another_model_indexes_anew() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    choose(&paul, MODEL, 0.15).await;
    api.embedder.refuse(&[
        ("illisible", LlmErrorKind::InvalidRequest),
        ("interminable", LlmErrorKind::ContextOverflow),
    ]);
    let odd = remember(
        &lucas,
        "Note illisible",
        "Un texte illisible pour le modèle.",
    )
    .await;
    let long = remember(&lucas, "Note interminable", "Un texte interminable.").await;
    let vans = remember(&lucas, VANS.0, VANS.1).await;

    index(&api).await;
    assert_eq!(vector_of(&api, odd).await, (Some(MODEL.into()), false));
    assert_eq!(vector_of(&api, long).await, (Some(MODEL.into()), false));
    assert_eq!(vector_of(&api, vans).await, (Some(MODEL.into()), true));
    assert!(
        !api.embedder
            .embedded()
            .iter()
            .any(|t| t.contains("illisible") || t.contains("interminable"))
    );
    assert_eq!(index(&api).await, 0);
    let shown = paul.get("/admin/embedding").await.json();
    let settings = &shown["settings"];
    assert_eq!(settings["indexingError"], json!(null));
    assert_eq!(
        settings["embedded"].as_i64().unwrap() + 2,
        settings["memories"].as_i64().unwrap()
    );

    // Another model: every memory is embedded again, the refused ones too.
    api.embedder.refuse(&[]);
    choose(&paul, "concepts-2", 0.15).await;
    assert_eq!(
        index(&api).await,
        usize::try_from(settings["memories"].as_i64().unwrap()).unwrap()
    );
    assert_eq!(
        vector_of(&api, odd).await,
        (Some("concepts-2".into()), true)
    );

    // Back to words: every vector is wiped, and the wipe audited.
    back_to_words(&paul).await;
    assert_eq!(vector_of(&api, vans).await, (None, false));
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let left: i64 =
        sqlx::query_scalar("select count(*) from memory_items where embedding_model is not null")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(left, 0);
    drop(tx);
    forget(&lucas, odd).await;
    forget(&lucas, long).await;
    forget(&lucas, vans).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_batch_refused_whole_is_the_model_s_failure_not_its_texts() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    choose(&paul, MODEL, 0.15).await;
    index(&api).await;
    let vans = remember(&lucas, VANS.0, VANS.1).await;
    let duty = remember(&lucas, "Astreinte", "Permanence du samedi matin.").await;

    // Every text refused, alone or not (a setting the model does not take):
    // none is marked refused, the failure is shown.
    api.embedder.refuse(&[("", LlmErrorKind::InvalidRequest)]);
    assert_eq!(index(&api).await, 0);
    assert_eq!(vector_of(&api, vans).await, (None, false));
    assert_eq!(vector_of(&api, duty).await, (None, false));
    let shown = paul.get("/admin/embedding").await.json();
    assert_eq!(shown["settings"]["indexingError"], "Texte refusé : ");

    api.embedder.refuse(&[]);
    assert_eq!(index(&api).await, 2);
    assert_eq!(vector_of(&api, vans).await, (Some(MODEL.into()), true));
    forget(&lucas, vans).await;
    forget(&lucas, duty).await;
    back_to_words(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pass_sends_nothing_once_the_choice_changed() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    choose(&paul, MODEL, 0.15).await;
    index(&api).await;
    // Changed where the service does not see it yet (its view is kept a
    // minute): the pass reads the choice again before sending.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update tenant_embedding_settings set model = 'concepts-2'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let vans = remember(&lucas, VANS.0, VANS.1).await;
    let asked = api.embedder.calls();
    assert_eq!(index(&api).await, 0);
    assert_eq!(api.embedder.calls(), asked);
    assert_eq!(vector_of(&api, vans).await, (None, false));
    forget(&lucas, vans).await;
    back_to_words(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn texts_too_long_are_refused_even_a_whole_batch_of_them() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    choose(&paul, MODEL, 0.15).await;
    index(&api).await;
    api.embedder
        .refuse(&[("interminable", LlmErrorKind::ContextOverflow)]);
    let first = remember(&lucas, "Note interminable", "Premier texte.").await;
    let second = remember(&lucas, "Autre note interminable", "Second texte.").await;

    // Every text of the batch too long: each refused, no failure.
    assert_eq!(index(&api).await, 0);
    assert_eq!(vector_of(&api, first).await, (Some(MODEL.into()), false));
    assert_eq!(vector_of(&api, second).await, (Some(MODEL.into()), false));
    // Alone in its batch too, and not asked again.
    let third = remember(&lucas, "Dernière note interminable", "Troisième.").await;
    assert_eq!(index(&api).await, 0);
    assert_eq!(vector_of(&api, third).await, (Some(MODEL.into()), false));
    let asked = api.embedder.calls();
    assert_eq!(index(&api).await, 0);
    assert_eq!(api.embedder.calls(), asked);
    let shown = paul.get("/admin/embedding").await.json();
    assert_eq!(shown["settings"]["indexingError"], json!(null));

    api.embedder.refuse(&[]);
    for memory in [first, second, third] {
        forget(&lucas, memory).await;
    }
    back_to_words(&paul).await;
}

/// Writes `count` personal memories of Lucas's straight into the store.
async fn notes_of_lucas(api: &TestApi, count: usize) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    for n in 0..count {
        create_memory(
            &mut tx,
            NewMemory {
                tenant_id: tenant::ID,
                scope: MemoryScope::User,
                owner_user_id: Some(user::LUCAS),
                org_unit_id: None,
                kind: MemoryKind::Fact,
                title: format!("Note de tournée {n}"),
                content: format!("Tournée numéro {n} du secteur nord."),
                sensitivity: Sensitivity::Internal,
                status: MemoryStatus::Active,
                source: MemorySource::manual(None),
                created_by_user_id: Some(user::LUCAS),
                created_by_run_id: None,
            },
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn going_back_to_words_mid_pass_stops_it_and_drops_what_it_brings() {
    let api = start().await;
    let paul = api.sign_in(user::PAUL).await;
    choose(&paul, MODEL, 0.15).await;
    index(&api).await;
    // More than a batch awaits.
    notes_of_lucas(&api, 40).await;

    api.embedder.hold();
    let services = api.app.services.clone();
    let pass = tokio::spawn(async move { services.embeddings.index_pending().await });
    api.embedder.entered().await;
    let asked = api.embedder.calls();
    back_to_words(&paul).await;
    api.embedder.release();
    pass.await.unwrap().unwrap();

    // The batch in flight is dropped, and no other is sent.
    assert_eq!(api.embedder.calls(), asked);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let kept: i64 =
        sqlx::query_scalar("select count(*) from memory_items where embedding_model is not null")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(kept, 0);
    sqlx::query("delete from memory_items where title like 'Note de tournée %'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_memory_written_is_indexed_in_the_background() {
    let api = start_indexing().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    choose(&paul, MODEL, 0.15).await;
    let vans = remember(&lucas, VANS.0, VANS.1).await;
    let mut indexed = vector_of(&api, vans).await;
    for _ in 0..100 {
        if indexed.1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        indexed = vector_of(&api, vans).await;
    }
    assert_eq!(indexed, (Some(MODEL.into()), true));
    forget(&lucas, vans).await;
    back_to_words(&paul).await;
}
