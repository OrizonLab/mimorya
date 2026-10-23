//! A collaborator's absence declared by an account administrator (ADR-078):
//! the same absence as their own, naming whom stands in for them, marked as
//! the administrator's, told to the collaborator by e-mail, audited; the
//! collaborator keeps the hand. Refusals: `admin_absence_refusals.rs`.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::delegations::{ABSENCE, absence_changes, absence_of, away, back, declare_for};
use support::mcp::capture_logs;
use support::{TestApi, email};

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
        absence_changes(&api, user::MARC).await,
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

    // One yet to begin is called off, not ended.
    let starts = Utc::now() + Duration::days(2);
    let later = json!({
        "delegateId": user::JULIEN,
        "startsAt": starts.to_rfc3339(),
        "endsAt": (starts + Duration::days(3)).to_rfc3339(),
    });
    let declared = paul.put(&absence_of(user::MARC), later).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    let ended = paul.delete(&absence_of(user::MARC)).await;
    assert_eq!(ended.status, StatusCode::OK, "{}", ended.text());
    let told = api.mail.sent().last().cloned().expect("an e-mail to Marc");
    assert_eq!(told.subject, "Paul Renaud a annulé votre absence");
    assert!(told.text.contains("Julien Moreau ne décidera pas"));
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
    let logs = capture_logs();
    let recorded = declare_for(&paul, user::MARC, user::JULIEN).await;
    let logged = logs.text();
    drop(logs);
    assert_eq!(recorded, json!({ "notified": false }));
    // The relay's answer may name the recipient: neither is logged.
    assert!(logged.contains("Absence e-mail not sent"), "{logged}");
    assert!(!logged.contains(&email(user::MARC)), "{logged}");
    assert!(!logged.contains("relais injoignable"), "{logged}");
    assert_eq!(
        marc.get(ABSENCE).await.json()["own"]["delegate"]["id"],
        json!(user::JULIEN)
    );
    let ended = paul.delete(&absence_of(user::MARC)).await;
    assert_eq!(ended.json(), json!({ "notified": false }));
}
