//! Who reads, deposits and deletes which documents (ADR-095), each rule
//! tried to be bypassed. A team's documents are read by its members and the
//! units above, never by a sibling team; the company's by everyone with
//! `document:read`; none above one's clearance. One deposits only where
//! `document:write` reaches, never above one's clearance, and deletes the
//! and replaces (ADR-150) the same way; what one may not delete or replace,
//! or not read, answers as absent. A
//! deny on `document:read` hides them all, one on `document:write` leaves
//! them read only. Another company's document, or unit, is absent. A
//! deposit or a replacement is refused before its file is read. A unit
//! holding documents is not deleted.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mimorya_core::demo::{unit, user};
use mimorya_db::repositories::documents::{NewDocument, insert_document};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::json;
use support::companies::create;
use support::documents::{Target, deposit, deposited, kept, listed, remove, replace};
use support::{Client, TestApi};
use uuid::Uuid;

const TEXT: &[u8] = b"Consigne : purger le circuit avant intervention.";

async fn absent(client: &Client<'_>, id: Uuid) {
    for url in [format!("/documents/{id}"), format!("/documents/{id}/file")] {
        let refused = client.get(&url).await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND, "{url}");
    }
    not_written(client, id).await;
}

/// The document `id` is neither deleted nor replaced by `client`: absent.
async fn not_written(client: &Client<'_>, id: Uuid) {
    let refused = client.delete(&format!("/documents/{id}")).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    let refused = replace(client, id, "Autre.txt", 1, TEXT).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
}

/// `client` reads the document `id`, and may neither delete nor replace it.
async fn read_only(client: &Client<'_>, id: Uuid) {
    let read = client.get(&format!("/documents/{id}")).await.json();
    assert_eq!(read["canWrite"], false);
    not_written(client, id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn rights_follow_the_space_and_the_clearance() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let nadia = api.sign_in(user::NADIA).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let marc = api.sign_in(user::MARC).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let north = Target::unit(unit::TEAM_NORTH);
    let team = deposited(&julien, "Nord.txt", north, TEXT).await;
    let confidential = deposited(
        &julien,
        "Nord confidentiel.txt",
        north.sensitive("confidential"),
        TEXT,
    )
    .await;
    let company = deposited(&claire, "Entreprise.txt", Target::default(), TEXT).await;
    let south = deposited(&nadia, "Sud.txt", Target::unit(unit::TEAM_SOUTH), TEXT).await;

    // Lucas, of the north team, cleared for internal data: his team's and
    // the company's documents, never a confidential one nor the south's.
    let seen = listed(&lucas).await;
    assert!(seen.contains(&team) && seen.contains(&company), "{seen:?}");
    assert!(
        !seen.contains(&confidential) && !seen.contains(&south),
        "{seen:?}"
    );
    absent(&lucas, confidential).await;
    absent(&lucas, south).await;
    // He reads his team's but may not delete nor replace it, nor deposit
    // anywhere.
    read_only(&lucas, team).await;
    assert!(listed(&julien).await.contains(&team));
    for into in [north, Target::default()] {
        let refused = deposit(&lucas, "Lucas.txt", into, TEXT).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    }
    let list = lucas.get("/documents").await.json();
    assert_eq!(list["spaces"], json!([]));
    assert_eq!(list["quota"], json!(null));

    // Julien deposits in his team's space, under his clearance only.
    for into in [
        Target::unit(unit::TEAM_SOUTH),
        Target::default(),
        north.sensitive("restricted"),
    ] {
        let refused = deposit(&julien, "Ailleurs.txt", into, TEXT).await;
        assert_eq!(
            refused.status,
            StatusCode::FORBIDDEN,
            "{into:?}: {}",
            refused.text()
        );
    }
    let seen = listed(&julien).await;
    assert!(seen.contains(&confidential) && !seen.contains(&south));
    // The company's document he reads, and may not delete nor replace.
    read_only(&julien, company).await;
    absent(&julien, south).await;

    // Marc, over the technical service, reads and deletes in both teams.
    let seen = listed(&marc).await;
    for id in [team, confidential, south, company] {
        assert!(seen.contains(&id), "{id}");
    }
    remove(&marc, team).await;
    remove(&marc, south).await;
    remove(&julien, confidential).await;
    remove(&claire, company).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deny_on_reading_documents_hides_them_all() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let julien = api.sign_in(user::JULIEN).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let id = deposited(&julien, "Nord.txt", Target::unit(unit::TEAM_NORTH), TEXT).await;
    let only_lucas = json!({
        "op": "eq",
        "left": { "attr": "subject.userId" },
        "right": { "value": user::LUCAS },
    });
    let created = paul
        .post(
            "/admin/policies",
            json!({
                "description": "Pas de documents",
                "permissions": ["document:read"],
                "condition": only_lucas,
            }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let policy = created.json()["id"].as_str().unwrap().to_string();
    assert!(listed(&lucas).await.is_empty());
    absent(&lucas, id).await;
    assert!(listed(&julien).await.contains(&id));
    let removed = paul.delete(&format!("/admin/policies/{policy}")).await;
    assert_eq!(removed.status, StatusCode::OK);
    assert!(listed(&lucas).await.contains(&id));
    remove(&julien, id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_documents_and_units_are_absent() {
    let api = TestApi::start().await;
    let claire = api.sign_in(user::CLAIRE).await;
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let foreign = insert_document(
        &mut tx,
        NewDocument {
            tenant_id: OTHER_TENANT_ID,
            scope: "tenant",
            org_unit_id: None,
            title: "Tarifs",
            file_name: "Tarifs.txt",
            kind: "text",
            sensitivity: "public",
            page_count: None,
            uploaded_by_user_id: OTHER_USER_ID,
            content: b"Tarif confidentiel",
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    absent(&claire, foreign).await;
    assert!(!listed(&claire).await.contains(&foreign));
    // Neither a unit of another company, though Claire deposits anywhere in
    // hers, nor one that does not exist.
    let acme = create(
        &api,
        "documents-ailleurs",
        "admin@documents-ailleurs.example",
    )
    .await;
    let mut tx = api.app.services.db.begin(acme.tenant_id).await.unwrap();
    let acme_unit: Uuid = sqlx::query_scalar("select id from org_units limit 1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let before = kept(&api).await;
    for unit in [acme_unit, Uuid::new_v4()] {
        let refused = deposit(&claire, "Ailleurs.txt", Target::unit(unit), TEXT).await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    }
    assert_eq!(kept(&api).await, before);
    let mut tx = api.app.services.db.begin(acme.tenant_id).await.unwrap();
    let theirs: i64 = sqlx::query_scalar("select count(*) from documents")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(theirs, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deny_on_writing_documents_leaves_them_read_only() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let julien = api.sign_in(user::JULIEN).await;
    let id = deposited(&julien, "Nord.txt", Target::unit(unit::TEAM_NORTH), TEXT).await;
    let only_julien = json!({
        "op": "eq",
        "left": { "attr": "subject.userId" },
        "right": { "value": user::JULIEN },
    });
    let created = paul
        .post(
            "/admin/policies",
            json!({
                "description": "Lecture seule",
                "permissions": ["document:write"],
                "condition": only_julien,
            }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let policy = created.json()["id"].as_str().unwrap().to_string();
    let refused = deposit(&julien, "Encore.txt", Target::unit(unit::TEAM_NORTH), TEXT).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    read_only(&julien, id).await;
    let list = julien.get("/documents").await.json();
    assert_eq!(list["spaces"], json!([]));
    assert_eq!(list["quota"], json!(null));
    let removed = paul.delete(&format!("/admin/policies/{policy}")).await;
    assert_eq!(removed.status, StatusCode::OK);
    remove(&julien, id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deposit_is_refused_before_its_file_is_read() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let julien = api.sign_in(user::JULIEN).await;
    // Without the request header: refused, as any write.
    let request = Request::post("/documents?name=a.txt")
        .header("cookie", format!("mimorya_session={}", julien.session))
        .body(Body::from(TEXT))
        .unwrap();
    assert_eq!(api.send(request).await.status, StatusCode::FORBIDDEN);
    // Lucas may not deposit: his file, however large, is not read.
    let large = vec![b'a'; 11 * 1024 * 1024];
    let refused = deposit(&lucas, "Gros.txt", Target::unit(unit::TEAM_NORTH), &large).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    // Nor replace his team's document: absent, the file not read either.
    let team = deposited(&julien, "Nord.txt", Target::unit(unit::TEAM_NORTH), TEXT).await;
    let refused = replace(&lucas, team, "Gros.txt", 1, &large).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    // Nor Julien without the request header.
    let request = Request::put(format!("/documents/{team}/file?name=a.txt&version=1"))
        .header("cookie", format!("mimorya_session={}", julien.session))
        .body(Body::from(TEXT))
        .unwrap();
    assert_eq!(api.send(request).await.status, StatusCode::FORBIDDEN);
    let read = julien.get(&format!("/documents/{team}")).await.json();
    assert_eq!(read["version"], 1);
    remove(&julien, team).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_unit_holding_documents_is_not_deleted() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let created = paul
        .post(
            "/admin/org-units",
            json!({ "parentId": unit::TECHNICAL, "name": "Équipe Est", "type": "team" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let east: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    let id = deposited(&claire, "Est.txt", Target::unit(east), TEXT).await;
    let refused = paul.delete(&format!("/admin/org-units/{east}")).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert!(
        refused.error_message().contains("1 document(s)"),
        "{}",
        refused.text()
    );
    remove(&claire, id).await;
    let deleted = paul.delete(&format!("/admin/org-units/{east}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}
