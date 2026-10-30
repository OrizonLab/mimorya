//! What becomes of the notices to managers in Teams (ADR-142): the bot
//! writes only in the personal conversation it last met the manager in,
//! at its Teams service; a manager linked before it was kept, who has not
//! written to the bot since, is written nothing, recorded so, and told of
//! the next requests once they write to it. A paused bot tells nobody
//! until it is enabled again, then tells of the requests still waiting;
//! Microsoft unreachable, the notice is tried again later; the bot blocked,
//! it is refused and not tried again.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeTeams;
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::approver_notices::{nadia_s_request, notice_of, notify, refuse, retry_notices_now};
use support::queue::as_owner;
use support::sso::SSO_DEPLOYMENT;
use support::teams::MARC_TEAMS;
use support::teams_conversations::{linked_teams, said_to, talk, talk_from};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_is_written_where_the_bot_last_met_them() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let last_service = || said_to(&teams, MARC_TEAMS.0).pop().unwrap().service;

    // Linked before the bot kept where it met him, and silent since:
    // written nothing, recorded so.
    as_owner(
        "update gateway_identities set service_url = null, conversation_id = null \
         where user_id = $1",
        user::MARC,
    )
    .await;
    let before = said_to(&teams, MARC_TEAMS.0).len();
    let (_, first) = nadia_s_request(&api, &paul, "09:00").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(
        notice_of(&api, first).await,
        Some((Some("skipped".into()), 1))
    );
    assert_eq!(said_to(&teams, MARC_TEAMS.0).len(), before);
    refuse(&api, &marc, first).await;

    // He writes to the bot: told of the next requests there.
    talk(&api, &teams, id, "a-10", MARC_TEAMS, "/aide").await;
    let (_, second) = nadia_s_request(&api, &paul, "10:00").await;
    assert_eq!(notify(&api).await, 1);
    assert_eq!(last_service(), teams.service_url());
    refuse(&api, &marc, second).await;

    // He writes from another region of the Teams service: told there.
    let elsewhere = teams.other_service_url();
    talk_from(&api, &teams, (id, &elsewhere), "a-11", MARC_TEAMS, "/aide").await;
    let (_, third) = nadia_s_request(&api, &paul, "11:00").await;
    assert_eq!(notify(&api).await, 1);
    assert_eq!(last_service(), elsewhere);
    refuse(&api, &marc, third).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_paused_bot_tells_of_the_requests_once_enabled_again() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;
    let path = format!("/admin/gateways/{id}");
    let before = said_to(&teams, MARC_TEAMS.0).len();

    // Paused: nothing tried, nothing recorded.
    let paused = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notice_of(&api, approval).await, None);
    assert_eq!(said_to(&teams, MARC_TEAMS.0).len(), before);
    // Enabled again: the request still waits, he is told of it.
    let enabled = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    assert_eq!(notify(&api).await, 1);
    assert_eq!(said_to(&teams, MARC_TEAMS.0).len(), before + 1);
    refuse(&api, &marc, approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_teams_does_not_take_is_tried_again_or_given_up() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    linked_teams(&api, &teams, (&paul, &marc), MARC_TEAMS, user::MARC).await;

    // Microsoft down: kept to try again, not before its time.
    let (_, first) = nadia_s_request(&api, &paul, "09:00").await;
    teams.unavailable(true);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notice_of(&api, first).await, Some((None, 1)));
    teams.unavailable(false);
    assert_eq!(notify(&api).await, 0);
    retry_notices_now(&api).await;
    assert_eq!(notify(&api).await, 1);
    assert_eq!(notice_of(&api, first).await, Some((Some("sent".into()), 2)));
    refuse(&api, &marc, first).await;

    // The bot blocked: refused, and not tried again.
    let written = teams.sent().len();
    let (_, second) = nadia_s_request(&api, &paul, "10:00").await;
    teams.block(MARC_TEAMS.0);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(
        notice_of(&api, second).await,
        Some((Some("refused".into()), 1))
    );
    assert_eq!(teams.sent().len(), written);
    refuse(&api, &marc, second).await;
}
