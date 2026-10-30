//! What becomes of the answers in Teams (ADR-140): each end or pause of a
//! run started there is answered once, in the person's personal
//! conversation with the bot; a paused bot, or a Teams identity no longer
//! the person's, is written nothing; what Teams refuses (the person blocked
//! the bot) is not tried again; Microsoft unreachable is tried again later,
//! a few times, then given up.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::gateways::MAX_ATTEMPTS;
use mimorya_connectors::testing::FakeTeams;
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::slack_conversations::answer;
use support::sso::SSO_DEPLOYMENT;
use support::teams::MARC_TEAMS;
use support::teams_conversations::{
    last_said, linked_teams, retry_teams_now, said_to, talk, teams_replies,
};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_paused_bot_or_a_removed_link_is_written_nothing() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let written = || said_to(&teams, MARC_TEAMS.0).len();

    // Paused before its run's end is answered.
    api.script(vec![ScriptedStep::text("Réponse A.")]);
    talk(&api, &teams, id, "a-10", MARC_TEAMS, "Question A").await;
    let path = format!("/admin/gateways/{id}");
    let paused = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let before = written();
    assert_eq!(answer(&api).await, 0);
    // Enabled again: not written later.
    let resumed = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(resumed.status, StatusCode::OK, "{}", resumed.text());
    assert_eq!(answer(&api).await, 0);
    assert_eq!(written(), before);

    // His link removed before its run's end is answered.
    api.script(vec![ScriptedStep::text("Réponse B.")]);
    talk(&api, &teams, id, "a-11", MARC_TEAMS, "Question B").await;
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(answer(&api).await, 0);
    assert_eq!(written(), before);
    assert_eq!(
        teams_replies(&api, MARC_TEAMS.0).await,
        [(Some("skipped".into()), 1), (Some("skipped".into()), 1)]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn microsoft_unreachable_is_tried_again_and_a_blocked_bot_never() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;

    // Unreachable: tried again later, then written.
    api.script(vec![ScriptedStep::text("Réponse A.")]);
    talk(&api, &teams, id, "a-10", MARC_TEAMS, "Question A").await;
    teams.unavailable(true);
    assert_eq!(answer(&api).await, 0);
    assert_eq!(teams_replies(&api, MARC_TEAMS.0).await, [(None, 1)]);
    // Not before its time.
    teams.unavailable(false);
    assert_eq!(answer(&api).await, 0);
    retry_teams_now(&api, MARC_TEAMS.0).await;
    assert_eq!(answer(&api).await, 1);
    assert!(last_said(&teams, MARC_TEAMS.0).starts_with("Réponse A\\."));
    assert_eq!(
        teams_replies(&api, MARC_TEAMS.0).await,
        [(Some("sent".into()), 2)]
    );

    // Unreachable at each attempt: given up.
    api.script(vec![ScriptedStep::text("Réponse B.")]);
    talk(&api, &teams, id, "a-11", MARC_TEAMS, "Question B").await;
    teams.unavailable(true);
    for _ in 0..MAX_ATTEMPTS {
        retry_teams_now(&api, MARC_TEAMS.0).await;
        assert_eq!(answer(&api).await, 0);
    }
    teams.unavailable(false);
    retry_teams_now(&api, MARC_TEAMS.0).await;
    assert_eq!(answer(&api).await, 0);
    assert_eq!(
        teams_replies(&api, MARC_TEAMS.0).await[1],
        (Some("failed".into()), MAX_ATTEMPTS)
    );

    // He blocked the bot: refused, never tried again.
    api.script(vec![ScriptedStep::text("Réponse C.")]);
    talk(&api, &teams, id, "a-12", MARC_TEAMS, "Question C").await;
    teams.block(MARC_TEAMS.0);
    assert_eq!(answer(&api).await, 0);
    retry_teams_now(&api, MARC_TEAMS.0).await;
    assert_eq!(answer(&api).await, 0);
    assert_eq!(
        teams_replies(&api, MARC_TEAMS.0).await[2],
        (Some("refused".into()), 1)
    );
}
