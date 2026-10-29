//! What becomes of the notices to managers in Telegram (ADR-137): a manager
//! whose Telegram account is linked to nothing is written nothing, recorded
//! so and never tried again; a paused bot tells nobody until it is enabled
//! again, then tells of the requests still waiting; Telegram unreachable,
//! the notice is tried again later; its HTML refused, it is said in plain
//! text with its buttons; the bot blocked, it is refused and not tried
//! again. A company with both a Slack application and a Telegram bot tells
//! a manager linked to both through each, once, what became of each notice
//! kept apart.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeSlack, FakeTelegram};
use mimorya_core::demo::user;
use serde_json::json;
use support::approver_notices::{
    direct_to, nadia_s_request, notice_of, notice_through, notify, refuse, retry_notices_now,
};
use support::slack_conversations::{MARC_SLACK, linked};
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{MARC_TELEGRAM, MARC_USERNAME, declare_telegram, declare_telegram_beside};
use support::telegram_conversations::{chat_of, last_said, link_to, linked_bot};
use support::{TestApi, email};

const MARC: (i64, &str) = (MARC_TELEGRAM, MARC_USERNAME);

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_linked_to_nothing_is_written_nothing() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    declare_telegram(&paul).await;
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;

    assert_eq!(notify(&api).await, 0);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(
        notice_of(&api, approval).await,
        Some((Some("skipped".into()), 1))
    );
    assert!(telegram.messages().is_empty(), "nothing written");
    refuse(&api, &marc, approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_paused_bot_tells_of_the_requests_once_enabled_again() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let path = format!("/admin/gateways/{id}");
    let before = chat_of(&telegram, MARC_TELEGRAM).len();

    // Paused: nothing tried, nothing recorded.
    let paused = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notice_of(&api, approval).await, None);
    assert_eq!(chat_of(&telegram, MARC_TELEGRAM).len(), before);
    // Enabled again: the request still waits, he is told of it.
    let enabled = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    assert_eq!(notify(&api).await, 1);
    assert_eq!(chat_of(&telegram, MARC_TELEGRAM).len(), before + 1);
    refuse(&api, &marc, approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_telegram_does_not_take_is_tried_again_said_plainly_or_given_up() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    linked_bot(&api, &telegram, (&paul, &marc), MARC).await;

    // Telegram down: kept to try again, not before its time.
    let (_, first) = nadia_s_request(&api, &paul, "09:00").await;
    telegram.unavailable(true);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notice_of(&api, first).await, Some((None, 1)));
    telegram.unavailable(false);
    assert_eq!(notify(&api).await, 0);
    retry_notices_now(&api).await;
    assert_eq!(notify(&api).await, 1);
    assert_eq!(notice_of(&api, first).await, Some((Some("sent".into()), 2)));
    refuse(&api, &marc, first).await;

    // Its HTML refused: said in plain text, its buttons kept.
    let (_, second) = nadia_s_request(&api, &paul, "10:00").await;
    telegram.refuse_html(true);
    assert_eq!(notify(&api).await, 1);
    telegram.refuse_html(false);
    let told = last_said(&telegram, MARC_TELEGRAM);
    assert!(!told.html);
    assert!(
        told.text
            .starts_with("Nadia Benali : l’agent attend votre validation pour une action."),
        "{}",
        told.text
    );
    let keyboard = told.keyboard.expect("its buttons");
    assert_eq!(
        keyboard["inline_keyboard"][0][0]["callback_data"],
        format!("v:{second}")
    );
    refuse(&api, &marc, second).await;

    // The bot blocked: refused, and not tried again.
    let written = telegram.messages().len();
    let (_, third) = nadia_s_request(&api, &paul, "11:00").await;
    telegram.block(MARC_TELEGRAM);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(
        notice_of(&api, third).await,
        Some((Some("refused".into()), 1))
    );
    assert_eq!(telegram.messages().len(), written);
    refuse(&api, &marc, third).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_linked_in_slack_and_telegram_is_told_in_each_once() {
    let (slack, telegram) = (FakeSlack::start().await, FakeTelegram::start().await);
    let api = TestApi::start_with_gateways(&SSO_DEPLOYMENT, &slack.url, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let in_slack = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let in_telegram = declare_telegram_beside(&paul).await;
    link_to(&api, &telegram, (in_telegram, 1), &marc, MARC).await;
    let before = chat_of(&telegram, MARC_TELEGRAM).len();
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;

    // Told in each, once.
    assert_eq!(notify(&api).await, 2);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(direct_to(&slack, MARC_SLACK).len(), 1);
    assert_eq!(chat_of(&telegram, MARC_TELEGRAM).len(), before + 1);
    let sent = Some((Some("sent".into()), 1));
    assert_eq!(notice_through(&api, in_slack, approval).await, sent);
    assert_eq!(notice_through(&api, in_telegram, approval).await, sent);
    refuse(&api, &marc, approval).await;

    // One unreachable: the other told, the first tried again alone.
    let (_, second) = nadia_s_request(&api, &paul, "10:00").await;
    telegram.unavailable(true);
    assert_eq!(notify(&api).await, 1);
    telegram.unavailable(false);
    let pending = Some((None, 1));
    assert_eq!(notice_through(&api, in_telegram, second).await, pending);
    retry_notices_now(&api).await;
    assert_eq!(notify(&api).await, 1);
    assert_eq!(direct_to(&slack, MARC_SLACK).len(), 2);
    let sent_again = Some((Some("sent".into()), 2));
    assert_eq!(notice_through(&api, in_telegram, second).await, sent_again);
    refuse(&api, &marc, second).await;
}
