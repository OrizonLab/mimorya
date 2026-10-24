//! The bounds of a deposit (ADR-095): a file beyond its size, announced or
//! found so, is not read, and other requests keep their own bound; the
//! query string is as strict as a body; two deposits near the company's
//! quota count each other; a reader that fails refuses the file, audited,
//! nothing kept; beyond the readings allowed at once, one is asked to wait.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mimorya_core::demo::{unit, user};
use mimorya_core::documents::limits::MAX_FILE_BYTES;
use serde_json::json;
use support::TestApi;
use support::documents::{Target, audited, deposit, deposit_url, kept, remove};
use support::impostor::Impostor;
use uuid::Uuid;

const NORTH: Target<'static> = Target {
    unit: Some(unit::TEAM_NORTH),
    sensitivity: None,
};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_file_beyond_its_bound_is_not_read() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let before = kept(&api).await;
    let large = vec![b'a'; MAX_FILE_BYTES + 1];
    let refused = deposit(&julien, "Gros.txt", NORTH, &large).await;
    assert_eq!(refused.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(refused.error_message(), "Le fichier dépasse 10 Mo.");
    // Announced beyond it: not read at all.
    let announced = Request::post(deposit_url("Annoncé.txt", NORTH))
        .header("cookie", format!("mimorya_session={}", julien.session))
        .header("x-mimorya-request", "1")
        .header("content-length", (20 * 1024 * 1024).to_string())
        .body(Body::from("court"))
        .unwrap();
    assert_eq!(
        api.send(announced).await.status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(kept(&api).await, before);
    // Other requests keep their own bound.
    let content = "a".repeat(2 * 1024 * 1024);
    let memory = julien
        .post(
            "/memory",
            json!({ "space": "personal", "kind": "fact", "title": "Gros", "content": content }),
        )
        .await;
    assert_eq!(memory.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn its_query_string_is_as_strict_as_a_body() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    let north = unit::TEAM_NORTH;
    for query in [
        format!("name=a.txt&unitId={north}&folder=x"),
        format!("unitId={north}"),
        format!("name=a.txt&unitId={north}&sensitivity=secret"),
        format!("name=&unitId={north}"),
        "name=a.txt&unitId=nord".to_string(),
    ] {
        let refused = julien
            .post_file(
                &format!("/documents?{query}"),
                b"texte".to_vec(),
                "text/plain",
            )
            .await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{query}: {}",
            refused.text()
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn two_deposits_near_the_quota_count_each_other() {
    let api = TestApi::start_with(&[("MIMORYA_DOCUMENT_QUOTA_MB", "1")]).await;
    let julien = api.sign_in(user::JULIEN).await;
    let file = vec![b'a'; 600 * 1024];
    let (first, second) = tokio::join!(
        deposit(&julien, "Premier.txt", NORTH, &file),
        deposit(&julien, "Second.txt", NORTH, &file),
    );
    let mut statuses = [first.status, second.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::CREATED, StatusCode::CONFLICT]);
    let (taken, refused) = if first.status == StatusCode::CREATED {
        (first, second)
    } else {
        (second, first)
    };
    assert!(
        refused.error_message().contains("quota"),
        "{}",
        refused.text()
    );
    let id: Uuid = taken.json()["id"].as_str().unwrap().parse().unwrap();
    let list = julien.get("/documents").await.json();
    assert_eq!(list["quota"]["quotaBytes"], 1024 * 1024);
    assert_eq!(list["quota"]["usedBytes"], file.len());
    remove(&julien, id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_reader_that_fails_refuses_the_file_audited() {
    let failing = Impostor::new("exit 3");
    let program = failing.program().display().to_string();
    let api = TestApi::start_with(&[("MIMORYA_EXTRACTOR", &program)]).await;
    let julien = api.sign_in(user::JULIEN).await;
    let before = kept(&api).await;
    let refused = deposit(&julien, "Note.txt", NORTH, b"Texte.").await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        refused.error_message().contains("illisible"),
        "{}",
        refused.text()
    );
    assert_eq!(kept(&api).await, before);
    let rejected = audited(&api, "document.rejected").await;
    let (resource, metadata) = rejected.last().expect("audited");
    assert_eq!(*resource, None);
    assert_eq!(*metadata, json!({ "kind": "text", "bytes": 6 }));
    let mut tx = api
        .app
        .services
        .db
        .begin(mimorya_core::demo::tenant::ID)
        .await
        .unwrap();
    let (outcome, reason): (String, Option<String>) = sqlx::query_as(
        "select outcome, reason from audit_log where action = 'document.rejected' \
         order by id desc limit 1",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        (outcome.as_str(), reason.as_deref()),
        ("failure", Some("reader_failed"))
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn beyond_the_readings_allowed_at_once_one_is_asked_to_wait() {
    let slow = Impostor::new("exec /bin/sleep 3");
    let program = slow.program().display().to_string();
    let api = TestApi::start_with(&[("MIMORYA_EXTRACTOR", &program)]).await;
    let julien = api.sign_in(user::JULIEN).await;
    let (one, two, three) = tokio::join!(
        deposit(&julien, "Un.txt", NORTH, b"Texte."),
        deposit(&julien, "Deux.txt", NORTH, b"Texte."),
        deposit(&julien, "Trois.txt", NORTH, b"Texte."),
    );
    let mut statuses = [one.status, two.status, three.status];
    statuses.sort();
    // Two read (and fail: the stand-in answers nothing), one waits.
    assert_eq!(
        statuses,
        [
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::SERVICE_UNAVAILABLE
        ]
    );
}
