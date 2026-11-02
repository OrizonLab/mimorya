//! Microsoft as a company's Teams bot reaches it (ADR-138), against the
//! local Microsoft: a token asked of Entra once and kept, for that secret
//! only; a refused secret said in French; a message written and a member
//! named only at a Teams service the bot may trust, nothing asked
//! elsewhere; the Bot Framework's keys read once for many activities, those
//! arriving together waiting for that read, those cut short going to its
//! end, and Microsoft unreachable, not asked again at once.

use std::time::Duration;

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
        proxy: None,
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

#[tokio::test]
async fn microsoft_unreachable_is_not_asked_again_at_once() {
    let teams = FakeTeams::start().await;
    let api = api(&teams);
    teams.unavailable(true);
    assert!(api.keys(Some(TEAMS_KEY_ID)).await.is_err());
    // Within thirty seconds: no key, and Microsoft not asked.
    assert_eq!(api.keys(Some(TEAMS_KEY_ID)).await, Ok(Vec::new()));
    assert_eq!(teams.configuration_requests(), 1);
}

#[tokio::test]
async fn activities_arriving_together_wait_for_the_one_read() {
    let teams = FakeTeams::start().await;
    let api = api(&teams);
    // As on a restart: none kept, two activities at once.
    let (first, second) = tokio::join!(api.keys(Some(TEAMS_KEY_ID)), api.keys(Some(TEAMS_KEY_ID)));
    for keys in [first.unwrap(), second.unwrap()] {
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].kid, TEAMS_KEY_ID);
    }
    assert_eq!(teams.key_reads(), 1);
}

#[tokio::test]
async fn activities_arriving_together_ask_an_unreachable_microsoft_once() {
    let teams = FakeTeams::start().await;
    let api = api(&teams);
    teams.unavailable(true);
    let (first, second) = tokio::join!(api.keys(Some(TEAMS_KEY_ID)), api.keys(Some(TEAMS_KEY_ID)));
    for keys in [first, second] {
        assert!(keys.unwrap_or_default().is_empty());
    }
    assert_eq!(teams.configuration_requests(), 1);
}

#[tokio::test]
async fn a_read_cut_short_goes_to_its_end() {
    let teams = FakeTeams::start().await;
    let api = api(&teams);
    teams.slow(Duration::from_secs(1));
    // The activity's connection closes once Microsoft is asked.
    tokio::select! {
        _ = api.keys(Some(TEAMS_KEY_ID)) => panic!("read before it was cut short"),
        () = until(|| teams.configuration_requests() == 1) => {}
    }
    // The next activity takes what that read kept, Microsoft not asked again.
    let keys = api.keys(Some(TEAMS_KEY_ID)).await.unwrap();
    assert_eq!(keys[0].kid, TEAMS_KEY_ID);
    assert_eq!(teams.configuration_requests(), 1);
    assert_eq!(teams.key_reads(), 1);
}

#[tokio::test]
async fn reads_cut_short_together_ask_microsoft_once() {
    let teams = FakeTeams::start().await;
    let api = api(&teams);
    teams.slow(Duration::from_secs(1));
    // Three activities' connections close while Microsoft is asked.
    tokio::select! {
        _ = api.keys(Some(TEAMS_KEY_ID)) => panic!("read before it was cut short"),
        () = until(|| teams.configuration_requests() == 1) => {}
    }
    for _ in 0..2 {
        let waiting = api.keys(Some(TEAMS_KEY_ID));
        let cut = tokio::time::timeout(Duration::from_millis(50), waiting).await;
        assert!(cut.is_err());
    }
    // None waiting when the read ends: the next activity takes what it kept.
    until(|| teams.key_reads() == 1).await;
    let keys = api.keys(Some(TEAMS_KEY_ID)).await.unwrap();
    assert_eq!(keys[0].kid, TEAMS_KEY_ID);
    assert_eq!(teams.configuration_requests(), 1);
    assert_eq!(teams.key_reads(), 1);
}

/// Once `done` holds, within five seconds.
async fn until(done: impl Fn() -> bool) {
    let waited = tokio::time::timeout(Duration::from_secs(5), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    waited.await.expect("not within five seconds");
}
