//! A collaborator's absence declared by an account administrator (ADR-078):
//! the same absence as their own, naming whom stands in for them, marked as
//! the administrator's, told to the collaborator by e-mail, audited; the
//! collaborator keeps the hand. Only company-wide `user:manage`, never on
//! one's own account, never for another company's or an inactive account.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::{Value, json};
use support::delegations::{ABSENCE, away, back};
use support::{Client, TestApi, email};
use uuid::Uuid;

fn absence_of(user_id: Uuid) -> String {
    format!("/admin/users/{user_id}/absence")
}

/// A week from now, standing in: `delegate`.
fn a_week(delegate: Uuid) -> Value {
    json!({ "delegateId": delegate, "endsAt": (Utc::now() + Duration::days(7)).to_rfc3339() })
}

async fn declare_for(admin: &Client<'_>, absent: Uuid, delegate: Uuid) -> Value {
    let declared = admin.put(&absence_of(absent), a_week(delegate)).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    declared.json()
}

/// How `client`'s `method` on the absence of `absent` is answered.
async fn status(client: &Client<'_>, method: &str, absent: Uuid) -> StatusCode {
    let path = absence_of(absent);
    match method {
        "GET" => client.get(&path).await.status,
        "PUT" => client.put(&path, a_week(user::NADIA)).await.status,
        _ => client.delete(&path).await.status,
    }
}

/// Absence changes audited on `user_id`: who made them, and the delegate.
async fn audited(api: &TestApi, user_id: Uuid) -> Vec<(String, Option<Uuid>, Option<Value>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(String, Option<Uuid>, Value)> = sqlx::query_as(
        "select action, actor_user_id, metadata from audit_log \
         where action like 'approval.delegation%' and resource_id = $1::text order by id",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(action, actor, metadata)| (action, actor, metadata.get("delegateId").cloned()))
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_declares_the_absence_of_a_collaborator_who_cannot() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    let shown = paul.get(&absence_of(user::MARC)).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    assert_eq!(shown.json()["absence"], Value::Null);
    let candidates = shown.json()["candidates"].clone();
    assert!(!candidates.to_string().contains(&user::MARC.to_string()));

    let recorded = declare_for(&paul, user::MARC, user::JULIEN).await;
    assert_eq!(recorded, json!({ "notified": true }));
    let shown = paul.get(&absence_of(user::MARC)).await.json()["absence"].clone();
    assert_eq!(shown["delegate"]["id"], json!(user::JULIEN));
    assert_eq!(shown["declaredBy"]["displayName"], "Paul Renaud");

    // Marc's own, as if he had declared it, but named the administrator's.
    let own = marc.get(ABSENCE).await.json()["own"].clone();
    assert_eq!(own["delegate"]["displayName"], "Julien Moreau");
    assert_eq!(own["declaredBy"]["id"], json!(user::PAUL));
    let covered = julien.get(ABSENCE).await.json()["standingInFor"].clone();
    assert_eq!(covered[0]["absent"]["id"], json!(user::MARC));

    // Told by e-mail: by whom, who stands in, where to end it.
    let sent = api.mail.sent();
    let told = sent.last().expect("an e-mail to Marc");
    assert_eq!(told.to, email(user::MARC));
    assert_eq!(told.subject, "Paul Renaud a déclaré votre absence");
    assert!(
        told.text
            .contains("Julien Moreau peut décider à votre place")
    );
    assert!(told.text.contains("/approvals\n"));

    // Marc keeps the hand: he ends it, or declares his own.
    back(&marc).await;
    declare_for(&paul, user::MARC, user::JULIEN).await;
    away(&marc, user::NADIA, 0).await;
    let own = marc.get(ABSENCE).await.json()["own"].clone();
    assert_eq!(own["declaredBy"], Value::Null);

    // Ended by the administrator, told too.
    let ended = paul.delete(&absence_of(user::MARC)).await;
    assert_eq!(ended.status, StatusCode::OK, "{}", ended.text());
    assert_eq!(ended.json(), json!({ "notified": true }));
    let told = api.mail.sent().last().cloned().expect("an e-mail to Marc");
    assert_eq!(told.subject, "Paul Renaud a mis fin à votre absence");
    assert!(told.text.contains("Nadia Benali ne décide plus"));
    assert_eq!(marc.get(ABSENCE).await.json()["own"], Value::Null);
    let again = paul.delete(&absence_of(user::MARC)).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND, "{}", again.text());

    let marc_id = Some(user::MARC);
    let paul_id = Some(user::PAUL);
    let julien_named = Some(json!(user::JULIEN));
    assert_eq!(
        audited(&api, user::MARC).await,
        [
            (
                "approval.delegation_set".into(),
                paul_id,
                julien_named.clone()
            ),
            ("approval.delegation_ended".into(), marc_id, None),
            ("approval.delegation_set".into(), paul_id, julien_named),
            (
                "approval.delegation_set".into(),
                marc_id,
                Some(json!(user::NADIA))
            ),
            ("approval.delegation_ended".into(), paul_id, None),
        ]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_account_administrators_declare_another_s_absence() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    let mails = api.mail.sent().len();
    // A manager without `user:manage` neither reads nor declares Julien's.
    for method in ["GET", "PUT", "DELETE"] {
        assert_eq!(
            status(&marc, method, user::JULIEN).await,
            StatusCode::FORBIDDEN
        );
        // Nor the administrator his own: it is declared in « Validations ».
        assert_eq!(
            status(&paul, method, user::PAUL).await,
            StatusCode::FORBIDDEN
        );
        // Another company's account is as unknown as an absent one.
        assert_eq!(
            status(&paul, method, OTHER_USER_ID).await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(&paul, method, Uuid::new_v4()).await,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(julien.get(ABSENCE).await.json()["own"], Value::Null);
    assert_eq!(paul.get(ABSENCE).await.json()["own"], Value::Null);

    // An absence that cannot hold, as one's own.
    let refused = |delegate: Uuid, ends_in: i64, expected: StatusCode, message: &'static str| {
        let paul = &paul;
        async move {
            let ends = (Utc::now() + Duration::days(ends_in)).to_rfc3339();
            let body = json!({ "delegateId": delegate, "endsAt": ends });
            let answer = paul.put(&absence_of(user::JULIEN), body).await;
            assert_eq!(answer.status, expected, "{}", answer.text());
            assert_eq!(answer.json()["error"]["message"], message);
        }
    };
    let invalid = StatusCode::BAD_REQUEST;
    let unknown = "Ce collègue n’existe pas ou son compte n’est pas actif.";
    let himself = "Désignez comme remplaçant un autre collaborateur que l’absent.";
    refused(user::JULIEN, 3, invalid, himself).await;
    refused(OTHER_USER_ID, 3, invalid, unknown).await;
    refused(Uuid::new_v4(), 3, invalid, unknown).await;
    refused(
        user::MARC,
        -1,
        invalid,
        "La fin de l’absence doit être à venir.",
    )
    .await;
    let too_long = "Une absence déclarée dure au plus 90 jours : déclarez la suite plus tard.";
    refused(user::MARC, 91, invalid, too_long).await;

    // A disabled account: an absence would change nothing.
    let account = format!("/admin/users/{}", user::JULIEN);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    let inactive = "Ce compte n’est pas actif : une absence serait sans effet.";
    refused(user::MARC, 3, StatusCode::CONFLICT, inactive).await;
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());

    // Nothing declared, nobody told, nothing audited but the refusals.
    assert_eq!(julien.get(ABSENCE).await.json()["own"], Value::Null);
    assert_eq!(api.mail.sent().len(), mails);
    assert!(audited(&api, user::JULIEN).await.is_empty());
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let refusals: Vec<(String, Option<Uuid>)> = sqlx::query_as(
        "select action, actor_user_id from audit_log where action like 'admin.%' \
         and metadata->>'route' = '/admin/users/{id}/absence' order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert!(refusals.contains(&("admin.refused".into(), Some(user::MARC))));
    assert!(refusals.contains(&("admin.not_found".into(), Some(user::PAUL))));
    assert!(refusals.contains(&("admin.unknown_reference".into(), Some(user::PAUL))));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_absence_holds_when_the_collaborator_cannot_be_told() {
    // Without a relay.
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let recorded = declare_for(&paul, user::MARC, user::JULIEN).await;
    assert_eq!(recorded, json!({ "notified": false }));
    assert_eq!(
        marc.get(ABSENCE).await.json()["own"]["declaredBy"]["id"],
        json!(user::PAUL)
    );
    back(&marc).await;

    // With one that fails.
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    api.mail.fail();
    let recorded = declare_for(&paul, user::MARC, user::JULIEN).await;
    assert_eq!(recorded, json!({ "notified": false }));
    assert_eq!(
        marc.get(ABSENCE).await.json()["own"]["delegate"]["id"],
        json!(user::JULIEN)
    );
    let ended = paul.delete(&absence_of(user::MARC)).await;
    assert_eq!(ended.json(), json!({ "notified": false }));
}
