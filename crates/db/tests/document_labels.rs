//! A document's classification as the store keeps it (ADR-213, D4): a
//! request only while the company's usage is not off, replaced by a later
//! version's and dropped at its own version only; a level raised, or labels
//! set, only where the document is still at the version and the level they
//! were decided on; one proposal per document, replaced by a later one. The
//! transaction is left uncommitted: it is rolled back.

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_db::TenantTx;
use mimorya_db::repositories::document_labels::{
    LabelProposal, LabelRequest, dismiss_label_proposal, drop_label_proposal, drop_label_request,
    label_proposals, label_requests, put_label_proposal, raise_document_sensitivity,
    request_document_labels, set_document_labels,
};
use mimorya_db::repositories::documents::{NewDocument, insert_document};
use uuid::Uuid;

async fn document(tx: &mut TenantTx) -> Uuid {
    insert_document(
        tx,
        NewDocument {
            tenant_id: tenant::ID,
            scope: "tenant",
            org_unit_id: None,
            title: "Contrat",
            file_name: "Contrat.txt",
            kind: "text",
            sensitivity: "internal",
            page_count: None,
            uploaded_by_user_id: user::CLAIRE,
            content: b"Contrat de maintenance.",
        },
    )
    .await
    .unwrap()
}

async fn set_usage(tx: &mut TenantTx, mode: &str) {
    sqlx::query(
        "insert into tenant_decision_usages (tenant_id, usage, mode, threshold) \
         values ($1, 'document_labels', $2, 0.5) \
         on conflict (tenant_id, usage) do update set mode = excluded.mode",
    )
    .bind(tenant::ID)
    .bind(mode)
    .execute(&mut **tx)
    .await
    .unwrap();
}

async fn level(tx: &mut TenantTx, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("select sensitivity, document_type from documents where id = $1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn requests_levels_and_proposals_hold_to_the_version_and_level_decided() {
    let db = support::connect(&support::urls(|_| {}).app).await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let id = document(&mut tx).await;
    let requested = |requests: Vec<LabelRequest>| {
        requests
            .into_iter()
            .filter(|request| request.document_id == id)
            .map(|request| request.version)
            .collect::<Vec<_>>()
    };

    // Off, nothing awaits; set, a later version's request replaces the
    // former, which can no longer drop it.
    set_usage(&mut tx, "off").await;
    assert!(
        !request_document_labels(&mut tx, tenant::ID, id, 1)
            .await
            .unwrap()
    );
    set_usage(&mut tx, "observe").await;
    assert!(
        request_document_labels(&mut tx, tenant::ID, id, 1)
            .await
            .unwrap()
    );
    assert!(
        request_document_labels(&mut tx, tenant::ID, id, 2)
            .await
            .unwrap()
    );
    assert_eq!(requested(label_requests(&mut tx, 100).await.unwrap()), [2]);
    drop_label_request(&mut tx, id, 1).await.unwrap();
    assert_eq!(requested(label_requests(&mut tx, 100).await.unwrap()), [2]);
    drop_label_request(&mut tx, id, 2).await.unwrap();
    assert!(requested(label_requests(&mut tx, 100).await.unwrap()).is_empty());

    // Raised only at the version and level classified.
    let raised = raise_document_sensitivity(&mut tx, id, 2, "internal", "restricted");
    assert!(!raised.await.unwrap());
    let raised = raise_document_sensitivity(&mut tx, id, 1, "confidential", "restricted");
    assert!(!raised.await.unwrap());
    assert!(
        raise_document_sensitivity(&mut tx, id, 1, "internal", "restricted")
            .await
            .unwrap()
    );
    assert_eq!(level(&mut tx, id).await, ("restricted".into(), None));
    // Set only at the level its writer was checked at.
    let set = set_document_labels(&mut tx, id, 1, "internal", Some("contract"), "public");
    assert!(!set.await.unwrap());
    assert!(
        !set_document_labels(&mut tx, id, 2, "restricted", None, "public")
            .await
            .unwrap()
    );
    let set = set_document_labels(&mut tx, id, 1, "restricted", Some("contract"), "public");
    assert!(set.await.unwrap());
    assert_eq!(
        level(&mut tx, id).await,
        ("public".into(), Some("contract".into()))
    );

    // One proposal per document, a later one in place of the former.
    let mut proposal = LabelProposal {
        document_id: id,
        version: 1,
        document_type: Some("invoice".into()),
        sensitivity: None,
        raised_from: Some("internal".into()),
        engine: "jev".into(),
    };
    put_label_proposal(&mut tx, tenant::ID, &proposal)
        .await
        .unwrap();
    proposal.version = 2;
    proposal.raised_from = None;
    proposal.sensitivity = Some("public".into());
    put_label_proposal(&mut tx, tenant::ID, &proposal)
        .await
        .unwrap();
    assert_eq!(
        label_proposals(&mut tx, &[id]).await.unwrap(),
        [proposal.clone()]
    );
    // Dismissed only at the version classified.
    assert!(!dismiss_label_proposal(&mut tx, id, 1).await.unwrap());
    assert!(dismiss_label_proposal(&mut tx, id, 2).await.unwrap());
    put_label_proposal(&mut tx, tenant::ID, &proposal)
        .await
        .unwrap();
    assert!(drop_label_proposal(&mut tx, id).await.unwrap());
    assert!(!drop_label_proposal(&mut tx, id).await.unwrap());
}
