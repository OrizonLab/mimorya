//! Depositing documents (ADR-095): each kind read through the extraction
//! process and cut into passages, never across a page; listed, read and
//! downloaded as deposited; deleted with its file and passages. Each step
//! audited, never with the document's name. A file refused says why, and
//! nothing of it is kept.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{unit, user};
use mimorya_extract::testing::{docx, pdf, protected_pdf};
use serde_json::json;
use support::TestApi;
use support::documents::{Target, audited, deposit, deposited, kept, listed, passages_of, remove};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_document_is_deposited_read_downloaded_and_deleted() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let file = pdf(&[
        "Purge : couper l'alimentation.",
        "Controle : relever la pression.",
    ]);
    let north = Target::unit(unit::TEAM_NORTH);
    let taken = deposit(&julien, "Purge chaudière.pdf", north, &file).await;
    assert_eq!(taken.status, StatusCode::CREATED, "{}", taken.text());
    assert_eq!(taken.json()["pageCount"], 2);
    assert_eq!(taken.json()["passages"], 2);
    let id: uuid::Uuid = taken.json()["id"].as_str().unwrap().parse().unwrap();

    let list = julien.get("/documents").await.json();
    let item = list["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|document| document["id"] == json!(id))
        .expect("listed")
        .clone();
    assert_eq!(item["title"], "Purge chaudière");
    assert_eq!(item["fileName"], "Purge chaudière.pdf");
    assert_eq!(item["kind"], "pdf");
    assert_eq!(item["scope"], "org_unit");
    assert_eq!(item["orgUnit"], "Équipe Nord");
    assert_eq!(item["sensitivity"], "internal");
    assert_eq!(item["byteSize"], file.len());
    assert_eq!(item["canWrite"], true);
    // Julien deposits in his team's space only.
    assert_eq!(
        list["spaces"],
        json!([{ "unitId": unit::TEAM_NORTH, "name": "Équipe Nord" }])
    );
    assert!(list["quota"]["usedBytes"].as_i64().unwrap() >= file.len() as i64);
    assert_eq!(julien.get(&format!("/documents/{id}")).await.json(), item);

    let download = julien.get(&format!("/documents/{id}/file")).await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.body.as_ref(), file.as_slice());
    assert_eq!(download.headers["content-type"], "application/pdf");
    assert_eq!(
        download.headers["content-disposition"],
        "attachment; filename=\"Purge chaudi_re.pdf\"; \
         filename*=UTF-8''Purge%20chaudi%C3%A8re.pdf"
    );
    assert_eq!(
        passages_of(&api, id).await,
        [
            (Some(1), None, "Purge : couper l'alimentation.".to_string()),
            (Some(2), None, "Controle : relever la pression.".to_string()),
        ]
    );

    remove(&julien, id).await;
    for url in [format!("/documents/{id}"), format!("/documents/{id}/file")] {
        assert_eq!(
            julien.get(&url).await.status,
            StatusCode::NOT_FOUND,
            "{url}"
        );
    }
    assert!(!listed(&julien).await.contains(&id));
    assert_eq!(passages_of(&api, id).await, []);
    let mut tx = api
        .app
        .services
        .db
        .begin(mimorya_core::demo::tenant::ID)
        .await
        .unwrap();
    let files: i64 =
        sqlx::query_scalar("select count(*) from document_files where document_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(files, 0);

    // Each step audited, never with the document's name.
    let id = Some(id.to_string());
    let uploaded = audited(&api, "document.uploaded").await;
    let (_, metadata) = uploaded.iter().find(|(r, _)| *r == id).expect("uploaded");
    assert_eq!(
        *metadata,
        json!({
            "scope": "org_unit",
            "orgUnitId": unit::TEAM_NORTH,
            "kind": "pdf",
            "bytes": file.len(),
            "pages": 2,
            "passages": 2,
            "sensitivity": "internal",
        })
    );
    for action in ["document.downloaded", "document.deleted"] {
        let entries = audited(&api, action).await;
        assert!(entries.iter().any(|(r, _)| *r == id), "{action}");
    }
    let mut tx = api
        .app
        .services
        .db
        .begin(mimorya_core::demo::tenant::ID)
        .await
        .unwrap();
    let named: i64 = sqlx::query_scalar(
        "select count(*) from audit_log where action like 'document.%' \
         and (metadata::text like '%Purge%' or coalesce(reason, '') like '%Purge%')",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(named, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_kind_is_read_and_cut_at_its_titles() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let north = Target::unit(unit::TEAM_NORTH);
    let word = docx(
        "<w:p><w:pPr><w:pStyle w:val=\"Titre1\"/></w:pPr><w:r><w:t>Consignes</w:t></w:r></w:p>\
         <w:p><w:r><w:t>Porter les gants.</w:t></w:r></w:p>",
    );
    let files: [(&str, Vec<u8>); 3] = [
        ("Consignes.docx", word),
        ("Consignes.md", b"# Consignes\n\nPorter les gants.".to_vec()),
        (
            "Consignes.txt",
            b"# Consignes\n\nPorter les gants.".to_vec(),
        ),
    ];
    for (name, bytes) in files {
        let id = deposited(&julien, name, north, &bytes).await;
        assert_eq!(
            passages_of(&api, id).await,
            [(
                None,
                Some("Consignes".to_string()),
                "# Consignes\n\nPorter les gants.".to_string()
            )],
            "{name}"
        );
        remove(&julien, id).await;
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_file_refused_says_why_and_nothing_of_it_is_kept() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let north = Target::unit(unit::TEAM_NORTH);
    let before = kept(&api).await;
    let refusals: [(&str, Vec<u8>, StatusCode, &str); 7] = [
        (
            "faux.pdf",
            b"<html>page</html>".to_vec(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "ne correspond pas à son extension",
        ),
        (
            "photo.png",
            b"\x89PNG".to_vec(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "n’est pas accepté",
        ),
        (
            "Secret.pdf",
            protected_pdf(&["Secret."], "motdepasse"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "protégé par un mot de passe",
        ),
        (
            "Scan.pdf",
            pdf(&[""]),
            StatusCode::UNPROCESSABLE_ENTITY,
            "aucun texte lisible",
        ),
        (
            "latin.txt",
            b"caf\xe9".to_vec(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "UTF-8",
        ),
        (
            "Ancien.doc",
            b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1".to_vec(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "ancien format",
        ),
        (
            "dossier/",
            b"texte".to_vec(),
            StatusCode::BAD_REQUEST,
            "Nom de fichier invalide",
        ),
    ];
    for (name, bytes, status, says) in refusals {
        let refused = deposit(&julien, name, north, &bytes).await;
        assert_eq!(refused.status, status, "{name}: {}", refused.text());
        assert!(
            refused.error_message().contains(says),
            "{name}: {}",
            refused.text()
        );
    }
    assert_eq!(kept(&api).await, before);
}
