//! The list of documents (ADR-095, ADR-097): the latest ones a reader
//! reads, `LISTED` at most, the list saying when older ones are left out;
//! and the largest file a deposit takes, which the page checks before
//! sending one.

mod support;

use mimorya_api::services::documents::LISTED;
use mimorya_core::demo::{tenant, user};
use mimorya_core::documents::limits::MAX_FILE_BYTES;
use mimorya_db::repositories::documents::{NewDocument, insert_document};
use serde_json::Value;
use support::{Client, TestApi};

const TITLE: &str = "Note de lot";

async fn list(client: &Client<'_>) -> Value {
    let list = client.get("/documents").await;
    assert!(list.status.is_success(), "{}", list.text());
    list.json()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_list_says_when_older_documents_are_left_out() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let shown = list(&lucas).await;
    assert_eq!(shown["truncated"], false);
    assert_eq!(shown["maxFileBytes"], MAX_FILE_BYTES as u64);

    // More of the company's documents than listed at once.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    for _ in 0..=LISTED {
        insert_document(
            &mut tx,
            NewDocument {
                tenant_id: tenant::ID,
                scope: "tenant",
                org_unit_id: None,
                title: TITLE,
                file_name: "Note de lot.txt",
                kind: "text",
                sensitivity: "internal",
                page_count: None,
                uploaded_by_user_id: user::CLAIRE,
                content: b"Une note.",
            },
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let shown = list(&lucas).await;
    assert_eq!(shown["truncated"], true);
    assert_eq!(shown["documents"].as_array().unwrap().len(), LISTED);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from documents where title = $1")
        .bind(TITLE)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(list(&lucas).await["truncated"], false);
}
