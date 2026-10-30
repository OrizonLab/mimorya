//! Microsoft as a company's Teams bot reaches it (ADR-138), against the
//! local Microsoft: a token asked of Entra once and kept, for that secret
//! only; a refused secret said in French; a message written and a member
//! named only at a Teams service the bot may trust, nothing asked
//! elsewhere; the Bot Framework's keys read once for many activities.

use mimorya_connectors::outbound::{NetworkPolicy, http_client};
use mimorya_connectors::teams::{TeamsApi, TeamsEndpoints, TeamsFailure};
use mimorya_connectors::testing::{
    FakeTeams, TEAMS_APP, TEAMS_DIRECTORY, TEAMS_KEY_ID, TEAMS_SECRET, conversation_of, member_id,
};
use mimorya_core::gateways::teams::TeamsApplication;

const MARC: &str = "5a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

fn api(teams: &FakeTeams) -> TeamsApi {
    let local = NetworkPolicy {
        allow_private_network: true,
        allow_stdio: false,
    };
    TeamsApi::new(
        TeamsEndpoints::local(&teams.url),
        http_client(local).unwrap(),
    )
    .unwrap()
}

fn app(secret: &str) -> TeamsApplication {
    TeamsApplication {
        app_id: TEAMS_APP.into(),
        directory_id: TEAMS_DIRECTORY.into(),
        client_secret: secret.into(),
    }
}

#[tokio::test]
async fn a_token_is_asked_once_and_kept_for_its_secret_only() {
    let teams = FakeTeams::start().await;
    let api = api(&teams);
    let conversation = (teams.service_url(), conversation_of(MARC));
    let to = (conversation.0.as_str(), conversation.1.as_str());
    api.send(&app(TEAMS_SECRET), to, "Un").await.unwrap();
    api.send(&app(TEAMS_SECRET), to, "Deux").await.unwrap();
    assert_eq!(teams.token_requests(), 1);
    let sent = teams.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].text, "Deux");
    assert_eq!(sent[1].app, TEAMS_APP);
    // Another secret: asked again, and refused.
    let wrong = app("Abc8Q~fictif.mauvais_secret-0000000000");
    let refused = api.send(&wrong, to, "Trois").await.unwrap_err();
    assert!(refused.credentials_refused(), "{refused:?}");
    assert_eq!(teams.token_requests(), 2);
    assert_eq!(teams.sent().len(), 2);
}

#[tokio::test]
async fn nothing_is_asked_of_a_service_the_bot_may_not_trust() {
    let teams = FakeTeams::start().await;
    let api = api(&teams);
    let untrusted = teams.untrusted_url();
    let conversation = conversation_of(MARC);
    let refused = api
        .send(&app(TEAMS_SECRET), (&untrusted, &conversation), "Bonjour")
        .await;
    assert_eq!(refused, Err(TeamsFailure::Untrusted));
    let member = api
        .member(
            &app(TEAMS_SECRET),
            (&untrusted, &conversation),
            &member_id(MARC),
        )
        .await;
    assert_eq!(member, Err(TeamsFailure::Untrusted));
    assert_eq!(teams.token_requests(), 0);
    assert!(teams.sent().is_empty());
}

#[tokio::test]
async fn a_member_is_named_as_teams_says_and_the_keys_read_once() {
    let teams = FakeTeams::start().await;
    let api = api(&teams);
    teams.add_person(MARC, "Marc Lefèvre", "marc.lefevre@neo-maintenance.example");
    let service = teams.service_url();
    let conversation = conversation_of(MARC);
    let member = api
        .member(
            &app(TEAMS_SECRET),
            (&service, &conversation),
            &member_id(MARC),
        )
        .await
        .unwrap();
    assert_eq!(member.aad_object_id.as_deref(), Some(MARC));
    assert_eq!(member.tenant_id.as_deref(), Some(TEAMS_DIRECTORY));
    assert_eq!(
        member.email.as_deref(),
        Some("marc.lefevre@neo-maintenance.example")
    );
    // Another conversation's member: unknown there.
    let other = api
        .member(
            &app(TEAMS_SECRET),
            (&service, &conversation_of(TEAMS_APP)),
            &member_id(MARC),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(other, TeamsFailure::Refused { status: 404, .. }),
        "{other:?}"
    );

    for _ in 0..3 {
        let keys = api.keys(Some(TEAMS_KEY_ID)).await.unwrap();
        assert_eq!(keys[0].kid, TEAMS_KEY_ID);
    }
    assert_eq!(teams.key_reads(), 1);
    // A key not published: read again at most every five minutes.
    api.keys(Some("autre-cle")).await.unwrap();
    assert_eq!(teams.key_reads(), 1);
}
