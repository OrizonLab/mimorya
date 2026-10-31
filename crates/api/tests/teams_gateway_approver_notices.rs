//! Requests awaiting a manager told in Teams (ADR-142): the manager linked
//! to the company's Teams bot is written, in their personal conversation
//! with it, a paused run's requests awaiting them, once, the actions that
//! may be validated there on a card with their exact parameters and
//! buttons; pressed, the request is decided as that manager, audited so,
//! the card written again without its buttons and the outcome said there.
//! While the manager is away, whom they named to stand in for them is told
//! instead, and decides. An action sent outside the company is told on no
//! card, and no press decides it; nor does another linked person's press
//! on a manager's card. A manager who lost the right to decide is written
//! nothing.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::gateways::APPROVAL_NOTIFIED;
use mimorya_connectors::testing::FakeTeams;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::approver_notices::{nadia_s_request, notice_of, notify, refuse};
use support::delegations::{away, back, julien_decides_for_the_south};
use support::manager_approvals::{listed, without};
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use support::sso::SSO_DEPLOYMENT;
use support::teams::{LEA_TEAMS, MARC_TEAMS, link_code};
use support::teams_conversations::{last_said, linked_teams, said_to};
use support::teams_validations::{buttons, card_text, choice, latest_card, message, press};
use support::{TestApi, email};
use uuid::Uuid;

/// Julien's Microsoft Entra object, as Teams names him.
const JULIEN_TEAMS: (&str, &str) = ("8d4e5f6a-7b8c-4d9e-8f0a-2b3c4d5e6f7a", "Julien Moreau");
/// What a button the bot did not show there is told.
const STALE: &str = "Ce bouton n’est plus valable : rien n’a été décidé\\.";

/// The metadata of each notice audited for the request.
async fn audited(api: &TestApi, approval: Uuid) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_scalar(
        "select metadata from audit_log where action = $1 and resource_id = $2 order by id",
    )
    .bind(APPROVAL_NOTIFIED)
    .bind(approval.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_manager_is_told_in_teams_and_validates_on_its_card() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;

    // Told once, in his personal conversation, the action on a card with
    // its exact parameters and its buttons.
    let before = said_to(&teams, MARC_TEAMS.0).len();
    assert_eq!(notify(&api).await, 1);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(said_to(&teams, MARC_TEAMS.0).len(), before + 1);
    let notice = latest_card(&teams, MARC_TEAMS.0);
    assert!(
        notice
            .text
            .starts_with("**Nadia Benali** : l’agent attend votre validation pour une action."),
        "{}",
        notice.text
    );
    assert!(
        notice.text.contains("([ouvrir les validations](http"),
        "{}",
        notice.text
    );
    let card = notice.card.clone().unwrap();
    assert!(card_text(&card).contains("proposalId"), "{card}");
    assert_eq!(
        buttons(&card),
        [
            ("Valider".to_owned(), choice(approval, true)),
            ("Refuser".to_owned(), choice(approval, false)),
        ]
    );
    assert_eq!(
        notice_of(&api, approval).await,
        Some((Some("sent".into()), 1))
    );
    let manager = json!({
        "gatewayId": id,
        "gateway": "teams",
        "approverId": user::MARC,
        "grouped": 1,
    });
    assert_eq!(audited(&api, approval).await, [manager]);

    // Validated in Teams: decided as him, audited so, the card written
    // again once without its buttons, saying so, the outcome said there.
    let since = latest_entry(&api).await;
    press(
        &api,
        &teams,
        (id, &notice.id),
        ("a-11", MARC_TEAMS),
        choice(approval, true),
    )
    .await;
    assert_eq!(
        last_said(&teams, MARC_TEAMS.0),
        "Validée par vous dans Teams\\. La demande reprend\\."
    );
    let rewritten = message(&teams, &notice.id);
    assert_eq!(rewritten.rewritten, 1);
    assert_eq!(
        rewritten.text,
        "**Nadia Benali** : l’action montrée ci\\-dessous est décidée\\."
    );
    let card = rewritten.card.unwrap();
    assert!(buttons(&card).is_empty(), "{card}");
    assert!(card_text(&card).contains("Validée."), "{card}");
    assert!(!card_text(&card).contains("à valider"), "{card}");
    let decided = listed(&marc, &run).await.expect("the request he decided");
    assert_eq!(decided["status"], "approved");
    assert_eq!(decided["decidedBy"]["id"], json!(user::MARC));
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("approval.approved"), "{audited}");
    assert!(audited.contains(r#""gateway":"teams""#), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_stand_in_is_told_and_decides_in_the_manager_s_absence() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let julien = api.sign_in(user::JULIEN).await;
    let assignment = julien_decides_for_the_south(&paul).await;
    away(&marc, user::JULIEN, 0).await;
    let id = linked_teams(&api, &teams, (&paul, &julien), JULIEN_TEAMS, user::JULIEN).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;

    assert_eq!(notify(&api).await, 1);
    let notice = latest_card(&teams, JULIEN_TEAMS.0);
    assert!(
        notice.text.contains(", en l’absence de Marc Lefèvre."),
        "{}",
        notice.text
    );
    let audit = json!({
        "gatewayId": id,
        "gateway": "teams",
        "approverId": user::MARC,
        "grouped": 1,
        "delegateId": user::JULIEN,
    });
    assert_eq!(audited(&api, approval).await, [audit]);
    press(
        &api,
        &teams,
        (id, &notice.id),
        ("a-11", JULIEN_TEAMS),
        choice(approval, false),
    )
    .await;
    assert_eq!(
        last_said(&teams, JULIEN_TEAMS.0),
        "Refusée par vous dans Teams\\. La demande reprend\\."
    );
    let decided = listed(&julien, &run).await.expect("the request he decided");
    assert_eq!(decided["status"], "rejected");
    assert_eq!(decided["decidedBy"]["id"], json!(user::JULIEN));
    assert!(buttons(&message(&teams, &notice.id).card.unwrap()).is_empty());

    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_sent_outside_the_company_is_told_on_no_card() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;
    // As an e-mail sent in her name would be.
    as_owner(
        "update approvals set risk = 'external' where id = $1",
        approval,
    )
    .await;

    assert_eq!(notify(&api).await, 1);
    let notice = said_to(&teams, MARC_TEAMS.0).pop().unwrap();
    assert_eq!(notice.card, None);
    assert!(
        notice
            .text
            .contains("Elle se valide dans l’application, qui en montre les paramètres exacts"),
        "{}",
        notice.text
    );
    assert!(!notice.text.contains("proposalId"), "{}", notice.text);

    // A press claiming a button on it decides nothing.
    press(
        &api,
        &teams,
        (id, &notice.id),
        ("a-11", MARC_TEAMS),
        choice(approval, true),
    )
    .await;
    assert_eq!(last_said(&teams, MARC_TEAMS.0), STALE);
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
    refuse(&api, &marc, approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_linked_person_s_press_on_a_manager_s_card_decides_nothing() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;
    assert_eq!(notify(&api).await, 1);
    let card = latest_card(&teams, MARC_TEAMS.0).id;

    // Léa, linked too, naming his card in her own conversation.
    teams.add_person(LEA_TEAMS.0, LEA_TEAMS.1, &email(user::LEA));
    let lea = api.sign_in(user::LEA).await;
    let code = link_code(&api, &teams, id, "l-1", LEA_TEAMS).await;
    let linked = lea
        .post("/account/gateways/link", json!({ "code": code }))
        .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());
    press(
        &api,
        &teams,
        (id, &card),
        ("l-2", LEA_TEAMS),
        choice(approval, true),
    )
    .await;
    assert_eq!(last_said(&teams, LEA_TEAMS.0), STALE);
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
    assert_eq!(buttons(&message(&teams, &card).card.unwrap()).len(), 2);
    refuse(&api, &marc, approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_who_lost_the_right_is_written_nothing() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;

    // Named when the request was made, he loses the right before its
    // notice: neither its parameters nor its buttons are written to him.
    let before = said_to(&teams, MARC_TEAMS.0).len();
    let policy = without(&paul, user::MARC, "approval:decide").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(said_to(&teams, MARC_TEAMS.0).len(), before);
    assert_eq!(
        notice_of(&api, approval).await,
        Some((Some("skipped".into()), 1))
    );
    assert!(audited(&api, approval).await.is_empty());
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    refuse(&api, &marc, approval).await;
}
