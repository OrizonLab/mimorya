//! What the Bot Framework sends a company's Teams bot (ADR-138), before any
//! session: proven by Microsoft's token alone (signed with a key it
//! publishes, endorsed for Teams, for the bot's application, naming the
//! activity's service, valid now); anything else answered as an unknown
//! gateway, nothing done. Only that path goes without the browser's header.
//! A person's own message to the bot, in their personal conversation in
//! the company's directory, is taken once: answered with a link to link
//! their account, in that conversation; anything else, a paused bot, or a
//! service not Microsoft's Teams, is taken and nothing written. The keys
//! are read once for many activities.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{
    FakeTeams, OTHER_TEAMS_APP, Signer, TEAMS_APP, conversation_of, signed_token,
};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::gateways::GATEWAYS;
use support::sso::SSO_DEPLOYMENT;
use support::teams::{
    MARC_TEAMS, declare_teams, deliver, deliver_signed, offered_code, post_activity,
};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_teams_activity_goes_without_the_header() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let id = Uuid::new_v4();
    // Reached: it answers as to an unknown gateway.
    let reached = post_activity(&api, id, None, &json!({})).await;
    assert_eq!(reached.status, StatusCode::NOT_FOUND);
    assert_eq!(reached.error_message(), "Passerelle introuvable");
    for path in [
        format!("/gateways/teams/{id}/"),
        format!("/gateways/teams/{}", id.to_string().to_uppercase()),
        format!("/gateways/teams/{}", id.simple()),
        format!("/gateways/teams//{id}"),
        "/admin/gateways/teams".to_owned(),
    ] {
        let request = axum::http::Request::post(&path)
            .header("content-type", "application/json")
            .body(axum::body::Body::from("{}"))
            .unwrap();
        let refused = api.send(request).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            refused.error_message(),
            "En-tête de requête manquant",
            "{path}"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_activity_proves_itself_by_microsoft_s_token_alone() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_teams(&paul).await;
    let activity = teams.message(TEAMS_APP, "2000", MARC_TEAMS, "Bonjour");
    let unknown = |response: support::TestResponse| {
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "{}",
            response.text()
        );
        assert_eq!(response.error_message(), "Passerelle introuvable");
    };

    // Another gateway, no token, or not a bearer one.
    unknown(deliver(&api, &teams, Uuid::new_v4(), &activity).await);
    unknown(post_activity(&api, id, None, &activity).await);
    let basic = format!("Basic {}", teams.token(TEAMS_APP));
    unknown(post_activity(&api, id, Some(&basic), &activity).await);
    // Signed by someone else, under Microsoft's key's name.
    let claims = teams.claims(TEAMS_APP);
    unknown(deliver_signed(&api, id, (Signer::Forger, &claims), &activity).await);
    // For another bot, outdated, or for another service.
    let other_bot = teams.claims(OTHER_TEAMS_APP);
    unknown(deliver_signed(&api, id, (Signer::BotFramework, &other_bot), &activity).await);
    let mut expired = claims.clone();
    expired["exp"] = json!(chrono::Utc::now().timestamp() - 3600);
    unknown(deliver_signed(&api, id, (Signer::BotFramework, &expired), &activity).await);
    let mut elsewhere = claims.clone();
    elsewhere["serviceurl"] = json!("https://smba.trafficmanager.net/emea/");
    unknown(deliver_signed(&api, id, (Signer::BotFramework, &elsewhere), &activity).await);
    // Not an activity naming its service.
    let token = format!("Bearer {}", teams.token(TEAMS_APP));
    unknown(post_activity(&api, id, Some(&token), &json!({ "type": "message" })).await);
    assert!(teams.sent().is_empty(), "nothing written");

    // Microsoft's own: taken, the person offered a link where they wrote.
    let taken = deliver(&api, &teams, id, &activity).await;
    assert_eq!(taken.status, StatusCode::OK, "{}", taken.text());
    let sent = teams.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].conversation, conversation_of(MARC_TEAMS.0));
    assert_eq!(sent[0].text_format.as_deref(), Some("markdown"));
    assert!(
        sent[0].text.contains("[relier mon compte]("),
        "{}",
        sent[0].text
    );
    assert!(!offered_code(&teams).is_empty());
    // The keys read once for all of them.
    assert_eq!(teams.key_reads(), 1);

    // A key Microsoft no longer endorses for Teams proves nothing.
    teams.endorse(&["skype"]);
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let next = teams.message(TEAMS_APP, "2001", MARC_TEAMS, "Bonjour");
    unknown(deliver(&api, &teams, id, &next).await);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_person_s_own_message_is_answered_once() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_teams(&paul).await;
    let ignored = |change: &dyn Fn(&mut Value)| {
        let mut activity = teams.message(
            TEAMS_APP,
            &format!("3{}", rand_suffix()),
            MARC_TEAMS,
            "Bonjour",
        );
        change(&mut activity);
        activity
    };
    for activity in [
        ignored(&|a| a["conversation"]["conversationType"] = json!("groupChat")),
        ignored(&|a| a["type"] = json!("conversationUpdate")),
        ignored(&|a| a["recipient"]["id"] = json!(format!("28:{OTHER_TEAMS_APP}"))),
        ignored(&|a| {
            a["channelData"]["tenant"]["id"] = json!("1a2b3c4d-5e6f-4789-8abc-def012345678")
        }),
    ] {
        let taken = deliver(&api, &teams, id, &activity).await;
        assert_eq!(taken.status, StatusCode::OK, "{}", taken.text());
    }
    assert!(teams.sent().is_empty(), "nothing written");

    // Answered once, whatever the Bot Framework sends again.
    let activity = teams.message(TEAMS_APP, "3100", MARC_TEAMS, "Bonjour");
    for _ in 0..2 {
        let taken = deliver(&api, &teams, id, &activity).await;
        assert_eq!(taken.status, StatusCode::OK, "{}", taken.text());
    }
    assert_eq!(teams.sent().len(), 1);

    // Paused: taken, nothing written.
    let paused = paul
        .patch(&format!("{GATEWAYS}/{id}"), json!({ "enabled": false }))
        .await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let later = teams.message(TEAMS_APP, "3101", MARC_TEAMS, "Bonjour");
    assert_eq!(
        deliver(&api, &teams, id, &later).await.status,
        StatusCode::OK
    );
    assert_eq!(teams.sent().len(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_service_not_microsoft_s_teams_is_never_written_to() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_teams(&paul).await;
    // Signed for that service, as a Microsoft gone astray would.
    let service = teams.untrusted_url();
    let mut activity = teams.message(TEAMS_APP, "4000", MARC_TEAMS, "Bonjour");
    activity["serviceUrl"] = json!(service);
    let mut claims = teams.claims(TEAMS_APP);
    claims["serviceurl"] = json!(service);
    let token = format!("Bearer {}", signed_token(Signer::BotFramework, &claims));
    let taken = post_activity(&api, id, Some(&token), &activity).await;
    assert_eq!(taken.status, StatusCode::OK, "{}", taken.text());
    assert!(teams.sent().is_empty(), "nothing written");
}

/// A suffix no other activity of the test has.
fn rand_suffix() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_owned()
}
