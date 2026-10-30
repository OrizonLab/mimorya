//! What a conversation held in Teams answers to, and whom (ADR-140): its
//! answers go to the Teams service its person last wrote from, a run's
//! paused in a conversation closed since too; a Teams identity linked
//! since to someone else follows them, never continuing the conversation
//! of the person before, whose run's end is written nothing; a personal
//! conversation opened twice at once keeps one conversation, the other
//! refused as busy.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::AuthIdentity;
use mimorya_api::services::runs::GatewayThread;
use mimorya_connectors::testing::{FakeTeams, conversation_of};
use mimorya_core::DomainError;
use mimorya_core::demo::{tenant, user};
use mimorya_core::gateways::GatewayKind;
use serde_json::json;
use support::manager_approvals::listed;
use support::planning::{apply_proposal, propose};
use support::slack_conversations::{answer, settle};
use support::sso::SSO_DEPLOYMENT;
use support::teams::{MARC_TEAMS, link_code};
use support::teams_conversations::{
    last_said, linked_teams, open_teams_conversation, said_to, talk, talk_from, teams_replies,
    teams_runs, teams_services,
};
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn answers_go_to_the_teams_service_the_person_last_wrote_from() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let (usual, elsewhere) = (teams.service_url(), teams.other_service_url());
    let last_service = || {
        said_to(&teams, MARC_TEAMS.0)
            .pop()
            .expect("a message")
            .service
    };

    // From the usual service: answered there.
    api.script(vec![ScriptedStep::text("Réponse A.")]);
    talk(&api, &teams, id, "a-10", MARC_TEAMS, "Question A").await;
    assert_eq!(answer(&api).await, 1);
    assert_eq!(last_service(), usual);

    // His next words from another region: the conversation moves there,
    // and is answered there.
    api.script(vec![ScriptedStep::text("Réponse B.")]);
    let from_elsewhere = (id, elsewhere.as_str());
    talk_from(
        &api,
        &teams,
        from_elsewhere,
        "a-11",
        MARC_TEAMS,
        "Question B",
    )
    .await;
    assert_eq!(
        teams_services(&api, MARC_TEAMS.0).await,
        std::slice::from_ref(&elsewhere)
    );
    assert_eq!(answer(&api).await, 1);
    assert_eq!(last_service(), elsewhere);
    assert!(last_said(&teams, MARC_TEAMS.0).starts_with("Réponse B\\."));

    // A run paused there, its conversation closed since from the usual
    // service: its end follows him there.
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal(""),
        ScriptedStep::text("Planning de INT-2052 appliqué."),
    ]);
    let asked = "/nouvelle Planifie INT-2052";
    talk_from(&api, &teams, from_elsewhere, "a-12", MARC_TEAMS, asked).await;
    let (run, _, status) = teams_runs(&api).await.remove(0);
    assert_eq!(status, "awaiting_approval");
    assert_eq!(answer(&api).await, 1);
    assert_eq!(last_service(), elsewhere);
    talk(&api, &teams, id, "a-13", MARC_TEAMS, "/nouvelle").await;
    assert_eq!(last_service(), usual);
    assert_eq!(
        teams_services(&api, MARC_TEAMS.0).await,
        [usual.clone(), usual.clone()]
    );
    let request = listed(&marc, &run.to_string()).await.expect("his request");
    let decided = marc
        .post(
            &format!("/approvals/{}/decision", request["id"].as_str().unwrap()),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    settle(&api).await;
    assert_eq!(answer(&api).await, 1);
    assert_eq!(last_service(), usual);
    let said = last_said(&teams, MARC_TEAMS.0);
    assert!(
        said.starts_with("Planning de INT\\-2052 appliqué\\."),
        "{said}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_teams_identity_linked_since_to_someone_else_follows_them() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let lea = api.sign_in(user::LEA).await;
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    api.script(vec![ScriptedStep::text("Réponse pour Marc.")]);
    talk(&api, &teams, id, "a-10", MARC_TEAMS, "Question de Marc").await;
    let his = open_teams_conversation(&api, id, MARC_TEAMS.0, user::MARC)
        .await
        .expect("his conversation");

    // He removes his link; Teams now names Léa's address for that Entra
    // object, which is linked to her since.
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    teams.add_person(MARC_TEAMS.0, MARC_TEAMS.1, &email(user::LEA));
    let code = link_code(&api, &teams, id, "a-11", MARC_TEAMS).await;
    let linked = lea
        .post("/account/gateways/link", json!({ "code": code }))
        .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());

    // `/nouvelle` from that identity closes nothing of his.
    talk(&api, &teams, id, "a-12", MARC_TEAMS, "/nouvelle").await;
    let still = open_teams_conversation(&api, id, MARC_TEAMS.0, user::MARC).await;
    assert_eq!(still, Some(his));

    // His run's end is not written there.
    let before = said_to(&teams, MARC_TEAMS.0).len();
    assert_eq!(answer(&api).await, 0);
    assert_eq!(said_to(&teams, MARC_TEAMS.0).len(), before);
    assert_eq!(
        teams_replies(&api, MARC_TEAMS.0).await,
        [(Some("skipped".into()), 1)]
    );

    // Its words follow Léa now, in a conversation of hers.
    api.script(vec![ScriptedStep::text("Réponse pour Léa.")]);
    talk(&api, &teams, id, "a-13", MARC_TEAMS, "Question de Léa").await;
    let hers = open_teams_conversation(&api, id, MARC_TEAMS.0, user::LEA)
        .await
        .expect("her conversation");
    assert_ne!(hers, his);
    let shown = lea.get(&format!("/conversations/{hers}")).await.json();
    assert_eq!(shown["messages"][0]["content"], "Question de Léa");
    let kept = marc.get(&format!("/conversations/{his}")).await.json();
    assert_eq!(kept["messages"].as_array().unwrap().len(), 2, "{kept}");
    assert_eq!(answer(&api).await, 1);
    let said = last_said(&teams, MARC_TEAMS.0);
    assert!(said.starts_with("Réponse pour Léa\\."), "{said}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_personal_conversation_opened_meanwhile_is_refused_as_busy() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    // Two messages finding no open conversation, the second started as
    // the first opened one: refused, nothing of it kept.
    let identity = AuthIdentity {
        tenant_id: tenant::ID,
        user_id: user::MARC,
        session_id: uuid::Uuid::nil(),
        must_change_password: false,
    };
    let (chat, service) = (conversation_of(MARC_TEAMS.0), teams.service_url());
    let opened_by = |activity: &'static str| GatewayThread {
        gateway_id: id,
        kind: GatewayKind::Teams,
        external_id: MARC_TEAMS.0,
        channel_id: &chat,
        thread_ts: activity,
        service_url: Some(&service),
    };
    let runs = &api.app.services.runs;
    api.script(vec![ScriptedStep::text("Réponse.")]);
    let title = "Conversation ouverte deux fois";
    runs.start_in_thread(identity, opened_by("a-10"), title, "Bonjour")
        .await
        .unwrap();
    let refused = runs
        .start_in_thread(identity, opened_by("a-11"), title, "Bonjour encore")
        .await
        .unwrap_err();
    let refusal = refused.downcast_ref::<DomainError>().expect("a refusal");
    assert!(
        refusal.message.contains("déjà en cours"),
        "{}",
        refusal.message
    );
    settle(&api).await;
    let listed = marc.get("/conversations").await.json();
    let titled = listed["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["title"] == title)
        .count();
    assert_eq!(titled, 1, "{listed}");
}
