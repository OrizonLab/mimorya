//! Validations given in Teams (ADR-141): a run started there that pauses
//! for its author's own validation shows the action on a card, with its
//! exact parameters and its buttons; pressed, it is decided as its author,
//! audited so, the card written again without its buttons and the outcome
//! said in the conversation, and the run goes on. An action decided
//! meanwhile in the application has its buttons taken off at the next
//! press, which decides nothing, but never by someone it is not for.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeTeams;
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::email;
use support::manager_approvals::listed;
use support::refusals::{entries_since, latest_entry};
use support::slack_conversations::answer;
use support::sso::SSO_DEPLOYMENT;
use support::teams::{MARC_TEAMS, link_code};
use support::teams_conversations::{last_said, linked_teams};
use support::teams_validations::{asked, buttons, card_text, choice, latest_card, message, press};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_awaiting_its_author_is_validated_on_its_card() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let end = "Planning de INT-2052 appliqué.";
    let (run, approval) = asked(&api, &teams, (id, MARC_TEAMS), end).await;

    // The pause: what to do, then the card of the action, its exact
    // parameters and its buttons.
    assert_eq!(answer(&api).await, 1);
    let pause = latest_card(&teams, MARC_TEAMS.0);
    assert!(
        pause.text.starts_with(
            "Une action de cette demande attend votre validation : validez\\-la ou \
             refusez\\-la ci\\-dessous"
        ),
        "{}",
        pause.text
    );
    let card = pause.card.clone().unwrap();
    let shown = card_text(&card);
    assert!(shown.contains("Action à valider : "), "{shown}");
    assert!(
        shown.contains("Paramètres exacts qui seront exécutés"),
        "{shown}"
    );
    assert!(shown.contains(r#""proposalId": ""#), "{shown}");
    assert_eq!(
        buttons(&card),
        [
            ("Valider".to_owned(), choice(approval, true)),
            ("Refuser".to_owned(), choice(approval, false)),
        ]
    );

    // Validated in Teams: decided as him, audited so, the card written
    // again once, without its buttons, saying so, the outcome said there.
    let since = latest_entry(&api).await;
    press(
        &api,
        &teams,
        (id, &pause.id),
        ("a-11", MARC_TEAMS),
        choice(approval, true),
    )
    .await;
    assert_eq!(
        last_said(&teams, MARC_TEAMS.0),
        "Validée par vous dans Teams\\. La demande reprend : la suite viendra ici\\."
    );
    let rewritten = message(&teams, &pause.id);
    assert!(
        rewritten
            .text
            .starts_with("L’action de cette demande montrée ci\\-dessous est décidée"),
        "{}",
        rewritten.text
    );
    assert_eq!(rewritten.rewritten, 1);
    let card = rewritten.card.unwrap();
    assert!(buttons(&card).is_empty(), "{card}");
    assert!(card_text(&card).contains("Décidée."), "{card}");
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "approved");
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("approval.approved"), "{audited}");
    assert!(audited.contains(r#""gateway":"teams""#), "{audited}");

    // The request went on: its end written in his conversation.
    assert_eq!(answer(&api).await, 1);
    assert!(last_said(&teams, MARC_TEAMS.0).starts_with("Planning de INT\\-2052 appliqué\\."));

    // Its button pressed again, as an old display may: nothing more decided.
    press(
        &api,
        &teams,
        (id, &pause.id),
        ("a-12", MARC_TEAMS),
        choice(approval, false),
    )
    .await;
    assert_eq!(
        last_said(&teams, MARC_TEAMS.0),
        "Cette demande a déjà été traitée"
    );
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "approved");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_refused_on_its_card_is_not_carried_out() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (run, approval) = asked(&api, &teams, (id, MARC_TEAMS), "Rien n’a été appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let pause = latest_card(&teams, MARC_TEAMS.0);
    press(
        &api,
        &teams,
        (id, &pause.id),
        ("a-11", MARC_TEAMS),
        choice(approval, false),
    )
    .await;
    let said = last_said(&teams, MARC_TEAMS.0);
    assert!(said.starts_with("Refusée par vous dans Teams\\."), "{said}");
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "rejected");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_decided_meanwhile_has_its_buttons_taken_off() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (run, approval) = asked(&api, &teams, (id, MARC_TEAMS), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let pause = latest_card(&teams, MARC_TEAMS.0);
    let decided = marc
        .post(
            &format!("/approvals/{approval}/decision"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    // Its button pressed in Teams then: nothing more decided, its buttons
    // taken off.
    press(
        &api,
        &teams,
        (id, &pause.id),
        ("a-11", MARC_TEAMS),
        choice(approval, false),
    )
    .await;
    assert_eq!(
        last_said(&teams, MARC_TEAMS.0),
        "Cette demande a déjà été traitée"
    );
    let rewritten = message(&teams, &pause.id);
    assert!(buttons(&rewritten.card.unwrap()).is_empty());
    assert_eq!(rewritten.rewritten, 1);
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "approved");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_decided_action_s_card_is_left_to_whom_it_is_for() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (_, approval) = asked(&api, &teams, (id, MARC_TEAMS), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let pause = latest_card(&teams, MARC_TEAMS.0);
    let path = format!("/approvals/{approval}/decision");
    let decided = marc.post(&path, json!({ "approved": true })).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    // His Teams account linked since to Léa: its former pause's buttons
    // pressed tell her nothing of his request, nor take them off.
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    teams.add_person(MARC_TEAMS.0, MARC_TEAMS.1, &email(user::LEA));
    let lea = api.sign_in(user::LEA).await;
    let code = link_code(&api, &teams, id, "a-20", MARC_TEAMS).await;
    let linked = lea
        .post("/account/gateways/link", json!({ "code": code }))
        .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());
    press(
        &api,
        &teams,
        (id, &pause.id),
        ("a-21", MARC_TEAMS),
        choice(approval, true),
    )
    .await;
    assert_eq!(
        last_said(&teams, MARC_TEAMS.0),
        "Demande de validation introuvable"
    );
    assert_eq!(buttons(&message(&teams, &pause.id).card.unwrap()).len(), 2);
}
