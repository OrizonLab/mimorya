//! What becomes of the notices to managers in Slack (ADR-130): a manager
//! linked to nothing, or whose Slack user Slack no longer says is theirs
//! (the identity unlinked), is written nothing, recorded so and never tried
//! again; a paused gateway tells nobody; Slack unreachable, the notice is
//! tried again later; buttons Slack refuses, the requests are told without
//! them; another refusal is not tried again.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeSlack;
use mimorya_core::demo::user;
use serde_json::json;
use support::gateways::declare_slack;
use support::slack_approvers::{
    direct_to, nadia_s_request, notice_of, notify, refuse, retry_notices_now,
};
use support::slack_conversations::{MARC_SLACK, linked};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_linked_to_nothing_or_no_longer_theirs_is_written_nothing() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let before = slack.posts().len();

    // His Slack user someone else's now: unlinked, told nothing.
    slack.add_person(MARC_SLACK, &email(user::LEA));
    let (_, first) = nadia_s_request(&api, &paul, "09:00").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(
        notice_of(&api, first).await,
        Some((Some("skipped".into()), 1))
    );
    let own = marc.get("/account/gateways").await.json();
    assert_eq!(own["gateways"][0]["linked"], serde_json::Value::Null);
    refuse(&api, &marc, first).await;
    // Linked to nothing: told nothing either, and never tried again.
    let (_, second) = nadia_s_request(&api, &paul, "10:00").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(
        notice_of(&api, second).await,
        Some((Some("skipped".into()), 1))
    );
    assert_eq!(slack.posts().len(), before, "nothing written");
    refuse(&api, &marc, second).await;

    // A paused gateway tells nobody.
    let path = format!("/admin/gateways/{gateway}");
    let paused = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let (_, third) = nadia_s_request(&api, &paul, "11:00").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notice_of(&api, third).await, None);
    refuse(&api, &marc, third).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_slack_does_not_take_is_tried_again_or_said_without_buttons() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;

    // Slack down: kept to try again, not before its time.
    let (_, first) = nadia_s_request(&api, &paul, "09:00").await;
    slack.unavailable(true);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notice_of(&api, first).await, Some((None, 1)));
    slack.unavailable(false);
    assert_eq!(notify(&api).await, 0);
    retry_notices_now(&api).await;
    assert_eq!(notify(&api).await, 1);
    assert_eq!(notice_of(&api, first).await, Some((Some("sent".into()), 2)));
    refuse(&api, &marc, first).await;

    // Its buttons refused: told without them.
    let (_, second) = nadia_s_request(&api, &paul, "10:00").await;
    slack.refuse_blocks(true);
    assert_eq!(notify(&api).await, 1);
    slack.refuse_blocks(false);
    let told = direct_to(&slack, MARC_SLACK).pop().unwrap();
    assert_eq!(told.blocks, None);
    assert!(
        told.text.contains("Elle se valide dans l’application"),
        "{}",
        told.text
    );
    assert_eq!(
        notice_of(&api, second).await,
        Some((Some("sent".into()), 1))
    );
    refuse(&api, &marc, second).await;

    // Another refusal: not tried again, nor without buttons.
    let before = slack.posts().len();
    let (_, third) = nadia_s_request(&api, &paul, "11:00").await;
    slack.refuse_with(Some("channel_not_found"));
    assert_eq!(notify(&api).await, 0);
    slack.refuse_with(None);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(
        notice_of(&api, third).await,
        Some((Some("refused".into()), 1))
    );
    assert_eq!(slack.posts().len(), before);
    refuse(&api, &marc, third).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_gateway_nothing_is_recorded() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let gateway = declare_slack(&paul).await;
    let removed = paul.delete(&format!("/admin/gateways/{gateway}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notice_of(&api, approval).await, None);
    refuse(&api, &marc, approval).await;
}
