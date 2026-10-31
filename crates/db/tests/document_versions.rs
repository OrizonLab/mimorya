//! A document replaced by a new version of its file (ADR-150): only while
//! it is at the version replaced, its file and passages with it, the
//! former passages' vectors gone; nothing else of a document, nor of its
//! file, the application may change. Each test leaves its transaction
//! uncommitted: it is rolled back.

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_core::documents::Passage;
use mimorya_db::repositories::document_versions::{NewVersion, replace_document_file};
use mimorya_db::repositories::documents::{
    NewDocument, insert_document, insert_passages, read_document_file,
};
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

fn passage(content: &str) -> Passage {
    Passage {
        page: None,
        heading: None,
        content: content.to_string(),
    }
}

/// A text document of the company, with one passage given a vector.
async fn document(tx: &mut TenantTx) -> Uuid {
    let id = insert_document(
        tx,
        NewDocument {
            tenant_id: tenant::ID,
            scope: "tenant",
            org_unit_id: None,
            title: "Consignes",
            file_name: "Consignes.txt",
            kind: "text",
            sensitivity: "internal",
            page_count: None,
            uploaded_by_user_id: user::CLAIRE,
            content: b"Purger le circuit.",
        },
    )
    .await
    .unwrap();
    insert_passages(tx, tenant::ID, id, &[passage("Purger le circuit.")])
        .await
        .unwrap();
    sqlx::query("update document_chunks set embedding_model = 'bge-m3' where document_id = $1")
        .bind(id)
        .execute(&mut **tx)
        .await
        .unwrap();
    id
}

const NEW_FILE: &[u8] = b"# Consignes\n\nPurger puis rincer le circuit.";

fn new_version<'a>(id: Uuid, replaces: i32, passages: &'a [Passage]) -> NewVersion<'a> {
    NewVersion {
        tenant_id: tenant::ID,
        document_id: id,
        replaces,
        title: "Consignes 2027",
        file_name: "Consignes 2027.md",
        kind: "markdown",
        page_count: None,
        replaced_by_user_id: user::JULIEN,
        content: NEW_FILE,
        passages,
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_version_still_current_is_replaced() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let id = document(&mut tx).await;
    let passages = [passage("Purger"), passage("puis rincer le circuit.")];

    // A version it is not at: nothing changes.
    for stale in [2, 0] {
        let replaced = replace_document_file(&mut tx, new_version(id, stale, &passages))
            .await
            .unwrap();
        assert_eq!(replaced, None);
    }
    let file = read_document_file(&mut tx, id).await.unwrap();
    assert_eq!(file.as_deref(), Some(&b"Purger le circuit."[..]));

    // The current one: the file, its description and its passages replaced,
    // the former passage's vector gone with it.
    let replaced = replace_document_file(&mut tx, new_version(id, 1, &passages))
        .await
        .unwrap();
    assert_eq!(replaced, Some(2));
    let (title, name, kind, size, version, replaced_by, replaced_at_set, created_before): (
        String,
        String,
        String,
        i32,
        i32,
        Option<Uuid>,
        bool,
        bool,
    ) = sqlx::query_as(
        "select title, file_name, kind, byte_size, version, replaced_by_user_id, \
         replaced_at is not null, created_at <= replaced_at from documents where id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        (title.as_str(), name.as_str(), kind.as_str()),
        ("Consignes 2027", "Consignes 2027.md", "markdown")
    );
    let new_size = NEW_FILE.len() as i32;
    assert_eq!(
        (size, version, replaced_by),
        (new_size, 2, Some(user::JULIEN))
    );
    assert!(replaced_at_set && created_before);
    let file = read_document_file(&mut tx, id).await.unwrap();
    assert_eq!(file.as_deref(), Some(NEW_FILE));
    let kept: Vec<(i32, String, Option<String>)> = sqlx::query_as(
        "select ordinal, content, embedding_model from document_chunks \
         where document_id = $1 order by ordinal",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        kept,
        [
            (0, "Purger".to_string(), None),
            (1, "puis rincer le circuit.".to_string(), None),
        ]
    );
    // The version it was at is gone.
    let again = replace_document_file(&mut tx, new_version(id, 1, &passages))
        .await
        .unwrap();
    assert_eq!(again, None);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_application_changes_nothing_else_of_a_document() {
    let db = app().await;
    let mut setup = db.begin(tenant::ID).await.unwrap();
    let id = document(&mut setup).await;
    setup.commit().await.unwrap();
    for change in [
        "scope = 'org_unit'",
        "org_unit_id = null",
        "sensitivity = 'public'",
        "uploaded_by_user_id = null",
        "created_at = now()",
        "tenant_id = tenant_id",
        "id = id",
    ] {
        let mut tx = db.begin(tenant::ID).await.unwrap();
        let update = sqlx::query(&format!("update documents set {change} where id = $1"))
            .bind(id)
            .execute(&mut *tx)
            .await;
        assert!(
            support::db_error(update).contains("permission denied"),
            "{change}"
        );
    }
    for change in ["document_id = document_id", "tenant_id = tenant_id"] {
        let mut tx = db.begin(tenant::ID).await.unwrap();
        let update = sqlx::query(&format!(
            "update document_files set {change} where document_id = $1"
        ))
        .bind(id)
        .execute(&mut *tx)
        .await;
        assert!(
            support::db_error(update).contains("permission denied"),
            "{change}"
        );
    }
    // A version beyond the first always says when it was replaced.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let unsaid = sqlx::query("update documents set version = 2 where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await;
    assert!(support::db_error(unsaid).contains("documents_replaced"));
    drop(tx);
    let mut tx = db.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from documents where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}
