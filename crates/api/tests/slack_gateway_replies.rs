//! What becomes of the answers in Slack (ADR-127): each end or pause of a
//! run started there is answered once; a paused gateway, or a Slack user no
//! longer the person's identity, is written nothing; what Slack refuses is
//! not tried again; Slack unreachable is tried again later, a few times,
//! then given up.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::gateways::MAX_ATTEMPTS;
use mimorya_connectors::testing::FakeSlack;
use mimorya_core::demo::user;
use serde_json::json;
use support::gateways::ACCOUNT;
use support::slack_conversations::{
    MARC_SLACK, answer, in_thread, linked, replies_of, retry_now, write, written,
};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email};

const FIRST: &str = "1700000200.000100";
const SECOND: &str = "1700000300.000100";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_paused_gateway_or_a_removed_identity_is_written_nothing() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;

    // Paused before its run's end is answered.
    api.script(vec![ScriptedStep::text("Réponse A.")]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question A", FIRST, None),
    )
    .await;
    let path = format!("/admin/gateways/{gateway}");
    let paused = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, FIRST).is_empty());
    assert_eq!(replies_of(&api, FIRST).await, [(Some("skipped".into()), 1)]);
    // Enabled again: not written later.
    let resumed = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(resumed.status, StatusCode::OK, "{}", resumed.text());
    assert_eq!(answer(&api).await, 0);

    // Unlinked by the person before the end is answered.
    api.script(vec![ScriptedStep::text("Réponse B.")]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question B", SECOND, None),
    )
    .await;
    let removed = marc.delete(&format!("{ACCOUNT}/{gateway}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, SECOND).is_empty());
    assert_eq!(
        replies_of(&api, SECOND).await,
        [(Some("skipped".into()), 1)]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_slack_refuses_is_not_tried_again() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.script(vec![ScriptedStep::text("Réponse.")]);
    write(&api, gateway, &written(MARC_SLACK, "Question", FIRST, None)).await;

    slack.refuse_with(Some("channel_not_found"));
    assert_eq!(answer(&api).await, 0);
    slack.refuse_with(None);
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, FIRST).is_empty());
    assert_eq!(replies_of(&api, FIRST).await, [(Some("refused".into()), 1)]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn slack_unreachable_is_tried_again_later_a_few_times() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.script(vec![
        ScriptedStep::text("Réponse A."),
        ScriptedStep::text("Réponse B."),
    ]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question A", FIRST, None),
    )
    .await;

    // Down: kept to try again, not before its time.
    slack.unavailable(true);
    assert_eq!(answer(&api).await, 0);
    assert_eq!(replies_of(&api, FIRST).await, [(None, 1)]);
    slack.unavailable(false);
    assert_eq!(answer(&api).await, 0);
    // Its time come, Slack up again: written.
    retry_now(&api, FIRST).await;
    assert_eq!(answer(&api).await, 1);
    assert_eq!(in_thread(&slack, FIRST).len(), 1);
    assert_eq!(replies_of(&api, FIRST).await, [(Some("sent".into()), 2)]);

    // Down at every attempt: given up.
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Question B", SECOND, None),
    )
    .await;
    slack.unavailable(true);
    for _ in 0..MAX_ATTEMPTS {
        assert_eq!(answer(&api).await, 0);
        retry_now(&api, SECOND).await;
    }
    slack.unavailable(false);
    assert_eq!(answer(&api).await, 0);
    assert!(in_thread(&slack, SECOND).is_empty());
    assert_eq!(
        replies_of(&api, SECOND).await,
        [(Some("failed".into()), MAX_ATTEMPTS)]
    );
}
