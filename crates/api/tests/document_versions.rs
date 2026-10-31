//! A document replaced by a new version of its file (ADR-150): the same
//! document, in its space at its sensitivity, first deposited when it was;
//! its file, name, title, kind, size and passages the new version's,
//! audited without its name. A replacement names the version it replaces:
//! of two made at once, one is taken, the other refused, and neither a past
//! nor a future version is replaced. A refused file, or request, leaves the
//! document as it was. Its bounds: `document_version_bounds.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{unit, user};
use mimorya_core::documents::limits::MAX_FILE_BYTES;
use serde_json::{Value, json};
use support::documents::{
    Target, audited, deposited, kept, passages_of, remove, replace, replace_url,
};
use support::{Client, TestApi};
use uuid::Uuid;

const NORTH: Target<'static> = Target {
    unit: Some(unit::TEAM_NORTH),
    sensitivity: None,
};

const FIRST: &[u8] = b"Purge : couper l'alimentation.";
const SECOND: &[u8] = b"Purge : couper puis vidanger le circuit.";

async fn item(client: &Client<'_>, id: Uuid) -> Value {
    let read = client.get(&format!("/documents/{id}")).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    read.json()
}

async fn file(client: &Client<'_>, id: Uuid) -> Vec<u8> {
    let download = client.get(&format!("/documents/{id}/file")).await;
    assert_eq!(download.status, StatusCode::OK);
    download.body.to_vec()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_new_version_replaces_the_file_keeping_the_document() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let into = NORTH.sensitive("confidential");
    let id = deposited(&julien, "Purge chaudière.txt", into, FIRST).await;
    let first = item(&julien, id).await;
    assert_eq!(
        (first["version"].clone(), first["replacedAt"].clone()),
        (json!(1), Value::Null)
    );
    let before = kept(&api).await;

    let replaced = replace(&julien, id, "Purge chaudière 2027.md", 1, SECOND).await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.text());
    assert_eq!(
        replaced.json(),
        json!({ "version": 2, "pageCount": null, "passages": 1 })
    );

    // The same document, in its space at its sensitivity, first deposited
    // when it was: its file and what describes it are the new version's.
    let second = item(&julien, id).await;
    assert_eq!(second["title"], "Purge chaudière 2027");
    assert_eq!(second["fileName"], "Purge chaudière 2027.md");
    assert_eq!(second["kind"], "markdown");
    assert_eq!(second["byteSize"], SECOND.len());
    assert_eq!(second["version"], 2);
    assert!(second["replacedAt"].is_string(), "{second}");
    for same in [
        "id",
        "scope",
        "orgUnitId",
        "orgUnit",
        "sensitivity",
        "createdAt",
    ] {
        assert_eq!(second[same], first[same], "{same}");
    }
    assert_eq!(second["canWrite"], true);
    assert_eq!(file(&julien, id).await, SECOND);
    assert_eq!(
        passages_of(&api, id).await,
        [(None, None, String::from_utf8(SECOND.to_vec()).unwrap())]
    );
    // One document and one file still; its passage replaced.
    assert_eq!(kept(&api).await, before);

    // Audited, never with its name.
    let entries = audited(&api, "document.replaced").await;
    let (_, metadata) = (entries.iter())
        .find(|(resource, _)| *resource == Some(id.to_string()))
        .expect("audited");
    assert_eq!(
        *metadata,
        json!({
            "version": 2,
            "kind": "markdown",
            "bytes": SECOND.len(),
            "pages": null,
            "passages": 1,
        })
    );
    assert!(!metadata.to_string().contains("Purge"));
    remove(&julien, id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_replacement_names_the_version_it_replaces() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let marc = api.sign_in(user::MARC).await;
    let id = deposited(&julien, "Purge.txt", NORTH, FIRST).await;

    // Julien and Marc replace version 1 at once: one is taken, the other
    // told the document was replaced in between.
    let (by_julien, by_marc) = tokio::join!(
        replace(&julien, id, "Purge Julien.txt", 1, b"Version de Julien."),
        replace(&marc, id, "Purge Marc.txt", 1, b"Version de Marc."),
    );
    let mut statuses = [by_julien.status, by_marc.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
    let (taken, refused, kept_file) = if by_julien.status == StatusCode::OK {
        (by_julien, by_marc, &b"Version de Julien."[..])
    } else {
        (by_marc, by_julien, &b"Version de Marc."[..])
    };
    assert_eq!(taken.json()["version"], 2);
    assert_eq!(
        refused.error_message(),
        "Ce document a été remplacé entre-temps par une autre version : vérifiez-la avant de la remplacer"
    );
    assert_eq!(file(&julien, id).await, kept_file);

    // Neither the version replaced, nor one to come, is replaced again.
    for version in [1, 3] {
        let refused = replace(&julien, id, "Purge.txt", version, SECOND).await;
        assert_eq!(refused.status, StatusCode::CONFLICT, "{version}");
    }
    assert_eq!(file(&julien, id).await, kept_file);
    assert_eq!(item(&julien, id).await["version"], 2);
    // The current one is.
    let replaced = replace(&julien, id, "Purge.txt", 2, SECOND).await;
    assert_eq!(replaced.json()["version"], 3, "{}", replaced.text());
    assert_eq!(file(&julien, id).await, SECOND);
    remove(&julien, id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_refused_replacement_leaves_the_document_as_it_was() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let id = deposited(&julien, "Purge.txt", NORTH, FIRST).await;
    let first = item(&julien, id).await;
    let before = kept(&api).await;

    let large = vec![b'a'; MAX_FILE_BYTES + 1];
    for (name, bytes, status) in [
        ("Purge.odt", FIRST, StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("Purge.doc", FIRST, StatusCode::UNPROCESSABLE_ENTITY),
        ("Purge.pdf", FIRST, StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("Purge.txt", &b"   "[..], StatusCode::UNPROCESSABLE_ENTITY),
        ("Purge.txt", &large[..], StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let refused = replace(&julien, id, name, 1, bytes).await;
        assert_eq!(refused.status, status, "{name}: {}", refused.text());
    }
    // Its query string is as strict as a body.
    for query in [
        "name=a.txt",
        "version=1",
        "name=&version=1",
        "name=a.txt&version=0",
        "name=a.txt&version=un",
        "name=a.txt&version=1&unitId=",
        "name=a.txt&version=1&sensitivity=public",
    ] {
        let refused = julien
            .put_file(
                &format!("/documents/{id}/file?{query}"),
                SECOND.to_vec(),
                "text/plain",
            )
            .await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{query}");
    }
    // A document that is not there.
    let refused = julien
        .put_file(
            &replace_url(Uuid::new_v4(), "a.txt", 1),
            SECOND.to_vec(),
            "text/plain",
        )
        .await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);

    assert_eq!(item(&julien, id).await, first);
    assert_eq!(file(&julien, id).await, FIRST);
    assert_eq!(kept(&api).await, before);
    let entries = audited(&api, "document.replaced").await;
    assert!(!entries.iter().any(|(r, _)| *r == Some(id.to_string())));
    remove(&julien, id).await;
}
