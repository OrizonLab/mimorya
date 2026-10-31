//! The Telegram messages kept to be written again once their actions are
//! settled (ADR-148): a pause's or a notice's, written with buttons, is kept
//! with the requests it shows. One written before it was kept, decided,
//! only has its buttons taken off, its text left as it was; its HTML
//! refused, it is written again in plain text. A notice's message is
//! forgotten once its requests are past, or once it is no longer the
//! private chat of the Telegram account linked to its manager (unlinked,
//! or another linked); a pause's never by that pass.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_connectors::testing::FakeTelegram;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::gateway_cards::purge_notice_cards;
use serde_json::json;
use support::TestApi;
use support::approver_notices::{nadia_s_request, notify, refuse};
use support::queue::as_owner;
use support::slack_conversations::answer;
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{MARC_TELEGRAM, MARC_USERNAME};
use support::telegram_conversations::{
    deliver, keyboard_of, last_brief, latest_number, link_to, linked_bot, message_numbered,
};
use support::telegram_validations::asked;

const MARC: (i64, &str) = (MARC_TELEGRAM, MARC_USERNAME);
/// Another Telegram account of Marc's.
const MARC_ELSEWHERE: (i64, &str) = (7272, "marc_ailleurs");

/// How many messages the company keeps: of notices (`notices`), or of
/// pauses.
async fn kept(api: &TestApi, notices: bool) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let column = if notices { "user_id" } else { "thread_id" };
    let count = sqlx::query_scalar(&format!(
        "select count(*) from gateway_cards where {column} is not null"
    ))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    count
}

/// Forgets the messages of notices as the maintenance pass does, eight
/// days on; how many.
async fn purge(api: &TestApi) -> i64 {
    let before = Utc::now() - Duration::days(8);
    purge_notice_cards(api.app.services.db.pool(), before)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_message_written_before_it_was_kept_only_has_its_buttons_taken_off() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (_, approval) = asked(&api, &telegram, (id, MARC), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    assert_eq!(kept(&api, false).await, 1);
    // As written before messages were kept.
    as_owner("delete from gateway_cards where gateway_id = $1", id).await;

    let press = telegram.press(11, MARC, number, &format!("v:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Validée");
    assert_eq!(
        keyboard_of(&telegram, number),
        json!({ "inline_keyboard": [] })
    );
    let pause = message_numbered(&telegram, number);
    assert_eq!(pause.edits, 0);
    assert!(
        pause.text.contains("<b>Action à valider :</b>"),
        "{}",
        pause.text
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn its_html_refused_a_message_is_written_again_in_plain_text() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (_, approval) = asked(&api, &telegram, (id, MARC), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    assert!(message_numbered(&telegram, number).html);

    telegram.refuse_html(true);
    let press = telegram.press(11, MARC, number, &format!("r:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Refusée");
    let rewritten = message_numbered(&telegram, number);
    assert_eq!(rewritten.edits, 1);
    assert!(!rewritten.html);
    assert!(
        rewritten
            .text
            .starts_with("L’action de cette demande montrée ci-dessous est décidée (http"),
        "{}",
        rewritten.text
    );
    assert!(
        rewritten.text.contains("\n\nAction : "),
        "{}",
        rewritten.text
    );
    assert!(rewritten.text.ends_with("\nRefusée."), "{}", rewritten.text);
    assert!(!rewritten.text.contains('<'), "{}", rewritten.text);
    telegram.refuse_html(false);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_notice_s_message_is_forgotten_once_past_or_no_longer_where_its_manager_is() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let notices = kept(&api, true).await;
    // A pause's message of his own, never forgotten by the pass.
    let (_, own) = asked(&api, &telegram, (id, MARC), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let pauses = kept(&api, false).await;
    assert!(pauses >= 1);
    refuse(&api, &marc, own).await;

    // Recent, in the chat of the account linked to him: kept.
    let (_, first) = nadia_s_request(&api, &paul, "09:00").await;
    assert_eq!(notify(&api).await, 1);
    assert_eq!(kept(&api, true).await, notices + 1);
    assert_eq!(purge(&api).await, 0);
    // Written longer ago than its requests could wait: forgotten, the
    // pause's message as old kept.
    as_owner(
        "update gateway_cards set created_at = now() - interval '9 days' where gateway_id = $1",
        id,
    )
    .await;
    assert_eq!(purge(&api).await, 1);
    assert_eq!(kept(&api, true).await, notices);
    assert_eq!(kept(&api, false).await, pauses);
    refuse(&api, &marc, first).await;

    // His Telegram account unlinked: forgotten at once.
    let (_, second) = nadia_s_request(&api, &paul, "10:00").await;
    assert_eq!(notify(&api).await, 1);
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(purge(&api).await, 1);
    assert_eq!(kept(&api, true).await, notices);
    refuse(&api, &marc, second).await;

    // Linked again, then from another Telegram account: the notice written
    // to the first is forgotten, the one written to the other kept.
    link_to(&api, &telegram, (id, 30), &marc, MARC).await;
    let (_, third) = nadia_s_request(&api, &paul, "11:00").await;
    assert_eq!(notify(&api).await, 1);
    refuse(&api, &marc, third).await;
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    link_to(&api, &telegram, (id, 40), &marc, MARC_ELSEWHERE).await;
    assert_eq!(kept(&api, true).await, notices + 1);
    assert_eq!(purge(&api).await, 1);
    assert_eq!(kept(&api, true).await, notices);
    let (_, fourth) = nadia_s_request(&api, &paul, "12:00").await;
    assert_eq!(notify(&api).await, 1);
    assert_eq!(kept(&api, true).await, notices + 1);
    assert_eq!(purge(&api).await, 0);
    refuse(&api, &marc, fourth).await;
}
