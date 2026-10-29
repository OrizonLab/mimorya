//! What becomes of the answers in Telegram (ADR-135): each end or pause of a
//! run started there is answered once, in the person's private chat with
//! the bot, in HTML; said in plain text when Telegram does not read the
//! HTML; a paused bot, or a Telegram account no longer the person's, is
//! written nothing; what Telegram refuses (the person blocked the bot) is
//! not tried again; Telegram unreachable is tried again later, a few times,
//! then given up.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::gateways::MAX_ATTEMPTS;
use mimorya_connectors::testing::FakeTelegram;
use mimorya_core::demo::user;
use serde_json::json;
use support::slack_conversations::answer;
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{MARC_TELEGRAM, MARC_USERNAME};
use support::telegram_conversations::{
    chat_of, chat_replies, last_said, linked_bot, retry_chat_now, talk, telegram_runs,
};
use support::{TestApi, WEB_ORIGIN};

const MARC: (i64, &str) = (MARC_TELEGRAM, MARC_USERNAME);

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn html_telegram_does_not_read_is_said_in_plain_text() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    api.script(vec![ScriptedStep::text("**Trois** visites <demain>.")]);
    talk(&api, &telegram, id, 10, MARC, "Quelles visites ?").await;
    let (_, conversation, _) = telegram_runs(&api).await.remove(0);
    telegram.refuse_html(true);
    assert_eq!(answer(&api).await, 1);
    let said = last_said(&telegram, MARC_TELEGRAM);
    assert!(!said.html);
    assert_eq!(
        said.text,
        format!(
            "**Trois** visites <demain>.\n\nDans l’application : {WEB_ORIGIN}/chat/{conversation}."
        )
    );
    assert_eq!(
        chat_replies(&api, MARC_TELEGRAM).await,
        [(Some("sent".into()), 1)]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_paused_bot_or_a_removed_link_is_written_nothing() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let written = || chat_of(&telegram, MARC_TELEGRAM).len();

    // Paused before its run's end is answered.
    api.script(vec![ScriptedStep::text("Réponse A.")]);
    talk(&api, &telegram, id, 10, MARC, "Question A").await;
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
    talk(&api, &telegram, id, 11, MARC, "Question B").await;
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(answer(&api).await, 0);
    assert_eq!(written(), before);
    assert_eq!(
        chat_replies(&api, MARC_TELEGRAM).await,
        [(Some("skipped".into()), 1), (Some("skipped".into()), 1)]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn telegram_unreachable_is_tried_again_and_a_blocked_bot_never() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;

    // Unreachable: tried again later, then written.
    api.script(vec![ScriptedStep::text("Réponse A.")]);
    talk(&api, &telegram, id, 10, MARC, "Question A").await;
    telegram.unavailable(true);
    assert_eq!(answer(&api).await, 0);
    assert_eq!(chat_replies(&api, MARC_TELEGRAM).await, [(None, 1)]);
    // Not before its time.
    telegram.unavailable(false);
    assert_eq!(answer(&api).await, 0);
    retry_chat_now(&api, MARC_TELEGRAM).await;
    assert_eq!(answer(&api).await, 1);
    assert!(
        last_said(&telegram, MARC_TELEGRAM)
            .text
            .starts_with("Réponse A.")
    );
    assert_eq!(
        chat_replies(&api, MARC_TELEGRAM).await,
        [(Some("sent".into()), 2)]
    );

    // Unreachable at each attempt: given up.
    api.script(vec![ScriptedStep::text("Réponse B.")]);
    talk(&api, &telegram, id, 11, MARC, "Question B").await;
    telegram.unavailable(true);
    for _ in 0..MAX_ATTEMPTS {
        retry_chat_now(&api, MARC_TELEGRAM).await;
        assert_eq!(answer(&api).await, 0);
    }
    telegram.unavailable(false);
    retry_chat_now(&api, MARC_TELEGRAM).await;
    assert_eq!(answer(&api).await, 0);
    assert_eq!(
        chat_replies(&api, MARC_TELEGRAM).await[1],
        (Some("failed".into()), MAX_ATTEMPTS)
    );

    // He blocked the bot: refused, never tried again.
    api.script(vec![ScriptedStep::text("Réponse C.")]);
    talk(&api, &telegram, id, 12, MARC, "Question C").await;
    telegram.block(MARC_TELEGRAM);
    assert_eq!(answer(&api).await, 0);
    retry_chat_now(&api, MARC_TELEGRAM).await;
    assert_eq!(answer(&api).await, 0);
    assert_eq!(
        chat_replies(&api, MARC_TELEGRAM).await[2],
        (Some("refused".into()), 1)
    );
}
