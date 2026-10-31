//! The bounds of a replacement by a new version (ADR-150): the company's
//! quota counts only what the new file adds, a smaller one taken even over
//! the quota; a deposit and a replacement near the quota count each other,
//! one taken, the other refused; a reader that fails refuses the new
//! version, audited with the document it was for, the document as it was.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, unit, user};
use mimorya_db::repositories::documents::{NewDocument, delete_document, insert_document};
use serde_json::{Value, json};
use support::TestApi;
use support::documents::{Target, audited, deposit, deposited, kept, remove, replace};
use support::impostor::Impostor;
use uuid::Uuid;

const NORTH: Target<'static> = Target {
    unit: Some(unit::TEAM_NORTH),
    sensitivity: None,
};

fn quota_used(list: &Value) -> i64 {
    list["quota"]["usedBytes"].as_i64().unwrap()
}

/// Lays in the store directly a text document of the company holding
/// `bytes`, as no deposit through the API could.
async fn laid(api: &TestApi, bytes: &[u8], scope: (&str, Option<Uuid>)) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id = insert_document(
        &mut tx,
        NewDocument {
            tenant_id: tenant::ID,
            scope: scope.0,
            org_unit_id: scope.1,
            title: "Archive",
            file_name: "Archive.txt",
            kind: "text",
            sensitivity: "internal",
            page_count: None,
            uploaded_by_user_id: user::JULIEN,
            content: bytes,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn unlaid(api: &TestApi, id: Uuid) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    assert!(delete_document(&mut tx, id).await.unwrap());
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_quota_counts_only_what_the_new_file_adds() {
    let api = TestApi::start_with(&[("MIMORYA_DOCUMENT_QUOTA_MB", "1")]).await;
    let julien = api.sign_in(user::JULIEN).await;
    // The demo documents count too.
    let demo = quota_used(&julien.get("/documents").await.json());
    let id = deposited(&julien, "Gros.txt", NORTH, &vec![b'a'; 600 * 1024]).await;

    // 900 KB in place of 600: within the quota, the former file not counted.
    let replaced = replace(&julien, id, "Gros.txt", 1, &vec![b'b'; 900 * 1024]).await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.text());
    let list = julien.get("/documents").await.json();
    assert_eq!(quota_used(&list), demo + 900 * 1024);
    // 1,100 KB would hold more than the quota: two bytes a character, read
    // within the characters a document may hold.
    let wide = "é".repeat(1100 * 512).into_bytes();
    let refused = replace(&julien, id, "Gros.txt", 2, &wide).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert!(
        refused.error_message().contains("quota"),
        "{}",
        refused.text()
    );

    // Over the quota (a document laid in the store directly), a smaller
    // file is still taken, never a larger one.
    let over = laid(&api, &vec![b'z'; 500 * 1024], ("tenant", None)).await;
    let refused = replace(&julien, id, "Gros.txt", 2, &vec![b'd'; 901 * 1024]).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    let smaller = replace(&julien, id, "Petit.txt", 2, &vec![b'e'; 100 * 1024]).await;
    assert_eq!(smaller.status, StatusCode::OK, "{}", smaller.text());
    assert_eq!(smaller.json()["version"], 3);

    unlaid(&api, over).await;
    remove(&julien, id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deposit_and_a_replacement_near_the_quota_count_each_other() {
    let api = TestApi::start_with(&[("MIMORYA_DOCUMENT_QUOTA_MB", "1")]).await;
    let julien = api.sign_in(user::JULIEN).await;
    let demo = quota_used(&julien.get("/documents").await.json());
    let id = deposited(&julien, "Petit.txt", NORTH, &vec![b'a'; 100 * 1024]).await;

    // 600 KB more either way: room for one only.
    let (larger, other) = (vec![b'b'; 700 * 1024], vec![b'c'; 600 * 1024]);
    let (replaced, deposited) = tokio::join!(
        replace(&julien, id, "Grand.txt", 1, &larger),
        deposit(&julien, "Autre.txt", NORTH, &other),
    );
    let taken = [
        replaced.status == StatusCode::OK,
        deposited.status == StatusCode::CREATED,
    ];
    assert!(
        taken[0] != taken[1],
        "{} / {}",
        replaced.text(),
        deposited.text()
    );
    let refused = if taken[0] { &deposited } else { &replaced };
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert!(
        refused.error_message().contains("quota"),
        "{}",
        refused.text()
    );
    let list = julien.get("/documents").await.json();
    assert_eq!(quota_used(&list), demo + 700 * 1024);
    if taken[1] {
        let other = deposited.json()["id"].as_str().unwrap().parse().unwrap();
        remove(&julien, other).await;
    }
    remove(&julien, id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_reader_that_fails_refuses_the_new_version_audited_with_its_document() {
    let failing = Impostor::new("exit 3");
    let program = failing.program().display().to_string();
    let api = TestApi::start_with(&[("MIMORYA_EXTRACTOR", &program)]).await;
    let julien = api.sign_in(user::JULIEN).await;
    let id = laid(
        &api,
        b"Purger le circuit.",
        ("org_unit", Some(unit::TEAM_NORTH)),
    )
    .await;
    let before = kept(&api).await;

    let refused = replace(&julien, id, "Purge.txt", 1, b"Purger puis rincer.").await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        refused.error_message().contains("illisible"),
        "{}",
        refused.text()
    );
    let read = julien.get(&format!("/documents/{id}")).await.json();
    assert_eq!(
        (read["version"].clone(), read["fileName"].clone()),
        (json!(1), json!("Archive.txt"))
    );
    assert_eq!(kept(&api).await, before);
    // Audited with the document it was for, never with the file's name.
    let rejected = audited(&api, "document.rejected").await;
    let (resource, metadata) = rejected.last().expect("audited");
    assert_eq!(*resource, Some(id.to_string()));
    assert_eq!(*metadata, json!({ "kind": "text", "bytes": 19 }));
    let replaced = audited(&api, "document.replaced").await;
    assert!(!replaced.iter().any(|(r, _)| *r == Some(id.to_string())));
    unlaid(&api, id).await;
}
