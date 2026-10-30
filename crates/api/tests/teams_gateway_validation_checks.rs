//! What a button pressed in Teams cannot decide (ADR-141): Teams sends
//! what the button carries as the client writes it, and the card it was
//! pressed on; only a request the platform keeps as shown on a card of its
//! own, in that personal conversation, is decided. A request the card does
//! not show, a message of the bot that is no card, a press naming no card,
//! another linked person's press naming a card of someone else's
//! conversation, a paused bot, a Teams account no longer linked decide
//! nothing. An action sent outside the company, or one a manager decides,
//! is shown on no card, and no press decides it.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeTeams, TEAMS_APP};
use mimorya_core::demo::user;
use serde_json::json;
use support::manager_approvals::{listed, route_to_managers};
use support::queue::as_owner;
use support::slack_conversations::{answer, settle};
use support::sso::SSO_DEPLOYMENT;
use support::teams::{LEA_TEAMS, MARC_TEAMS, deliver, link_code};
use support::teams_conversations::{last_said, linked_teams, said_to, talk};
use support::teams_validations::{asked, choice, latest_card, press};
use support::{TestApi, email};
use uuid::Uuid;

/// Nadia's Microsoft Entra object, as Teams names her.
const NADIA_TEAMS: (&str, &str) = ("7c3d4e5f-6a7b-4c8d-9e0f-1a2b3c4d5e6f", "Nadia Benali");
/// What a button the bot did not show there is told.
const STALE: &str = "Ce bouton n’est plus valable : rien n’a été décidé\\.";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_request_shown_on_a_card_of_the_bot_s_own_there_is_decided() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (run, approval) = asked(&api, &teams, (id, MARC_TEAMS), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let card = latest_card(&teams, MARC_TEAMS.0).id;
    let pending = || async { listed(&marc, &run).await.unwrap()["status"].clone() };
    let said = || last_said(&teams, MARC_TEAMS.0);

    // A request the card does not show, as a client may claim.
    let other = Uuid::new_v4();
    press(
        &api,
        &teams,
        (id, &card),
        ("a-11", MARC_TEAMS),
        choice(other, true),
    )
    .await;
    assert_eq!(said(), STALE);
    // A message of the bot that is no card.
    talk(&api, &teams, id, "a-12", MARC_TEAMS, "/aide").await;
    let help = said_to(&teams, MARC_TEAMS.0).pop().unwrap().id;
    press(
        &api,
        &teams,
        (id, &help),
        ("a-13", MARC_TEAMS),
        choice(approval, true),
    )
    .await;
    assert_eq!(said(), STALE);
    // A press naming no card.
    let mut unanswering = teams.press(
        (TEAMS_APP, "a-14"),
        MARC_TEAMS,
        (&card, choice(approval, true)),
    );
    unanswering.as_object_mut().unwrap().remove("replyToId");
    assert_eq!(
        deliver(&api, &teams, id, &unanswering).await.status,
        StatusCode::OK
    );
    settle(&api).await;
    assert_eq!(said(), STALE);
    assert_eq!(pending().await, "pending");

    // Léa, linked too, naming his card in her own conversation: nothing
    // told of his request.
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
    assert_eq!(pending().await, "pending");

    // The bot paused: nothing taken, nothing said.
    let path = format!("/admin/gateways/{id}");
    let paused = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let written = teams.sent().len();
    press(
        &api,
        &teams,
        (id, &card),
        ("a-15", MARC_TEAMS),
        choice(approval, true),
    )
    .await;
    assert_eq!(teams.sent().len(), written);
    let resumed = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(resumed.status, StatusCode::OK, "{}", resumed.text());

    // His Teams account no longer linked: offered to link it, nothing
    // decided.
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    press(
        &api,
        &teams,
        (id, &card),
        ("a-16", MARC_TEAMS),
        choice(approval, true),
    )
    .await;
    assert!(
        said().contains("reliez votre compte Teams à votre compte"),
        "{}",
        said()
    );
    assert_eq!(pending().await, "pending");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_sent_outside_the_company_is_on_no_card_and_no_press_decides_it() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (run, approval) = asked(&api, &teams, (id, MARC_TEAMS), "Appliqué.").await;
    // As an e-mail sent in his name would be.
    as_owner(
        "update approvals set risk = 'external' where id = $1",
        approval,
    )
    .await;
    assert_eq!(answer(&api).await, 1);
    let pause = said_to(&teams, MARC_TEAMS.0).pop().unwrap();
    assert_eq!(pause.card, None);
    assert!(
        pause.text.contains("à donner dans l’application"),
        "{}",
        pause.text
    );
    // A press claiming a button on it decides nothing.
    press(
        &api,
        &teams,
        (id, &pause.id),
        ("a-11", MARC_TEAMS),
        choice(approval, true),
    )
    .await;
    assert_eq!(last_said(&teams, MARC_TEAMS.0), STALE);
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_a_manager_decides_is_on_no_card_and_no_press_of_hers_decides_it() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, nadia) = (
        api.sign_in(user::PAUL).await,
        api.sign_in(user::NADIA).await,
    );
    let id = linked_teams(&api, &teams, (&paul, &nadia), NADIA_TEAMS, user::NADIA).await;
    // Her manager, Marc, validates what she writes.
    route_to_managers(&paul, &["write"]).await;
    let (run, approval) = asked(&api, &teams, (id, NADIA_TEAMS), "Appliqué.").await;
    route_to_managers(&paul, &[]).await;
    assert_eq!(answer(&api).await, 1);
    let pause = said_to(&teams, NADIA_TEAMS.0).pop().unwrap();
    assert_eq!(pause.card, None);
    assert!(
        pause.text.contains("validation de votre responsable"),
        "{}",
        pause.text
    );
    // Her pressing a button claimed on it, either one, decides nothing.
    for (activity, approved) in [("a-11", true), ("a-12", false)] {
        let claimed = choice(approval, approved);
        press(
            &api,
            &teams,
            (id, &pause.id),
            (activity, NADIA_TEAMS),
            claimed,
        )
        .await;
        assert_eq!(last_said(&teams, NADIA_TEAMS.0), STALE);
        assert_eq!(listed(&nadia, &run).await.unwrap()["status"], "pending");
    }
}
