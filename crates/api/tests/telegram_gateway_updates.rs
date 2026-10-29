//! What Telegram sends the company's bot (ADR-133), before any session:
//! only an update carrying the webhook's secret back is read; an unknown
//! gateway and a missing or wrong secret answer alike, and so does a
//! Slack gateway's address. An update Telegram sends again is taken once;
//! anything but a person's own message in their private chat, and anything
//! sent to a paused bot, is taken and ignored. Someone who has not linked
//! their account is told how to, in their private chat, never what they
//! wrote. Only the exact path goes without the header the browser's
//! requests carry.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeTelegram, TELEGRAM_BOT, update};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::gateways::GATEWAYS;
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{LEA_TELEGRAM, LEA_USERNAME, declare_telegram, post_update, send_update};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_what_carries_the_webhook_s_secret_back_is_read() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_telegram(&paul).await;
    let (_, secret) = telegram.webhook(TELEGRAM_BOT).unwrap();
    let path = format!("/gateways/telegram/{id}");
    let body = update(1, LEA_TELEGRAM, LEA_USERNAME, "Le planning de demain ?");

    // No secret, another one, another gateway, or Slack's address: alike.
    let wrong = "b".repeat(64);
    for (path, sent) in [
        (path.clone(), None),
        (path.clone(), Some(wrong.as_str())),
        (path.clone(), Some("")),
        (
            format!("/gateways/telegram/{}", Uuid::new_v4()),
            Some(secret.as_str()),
        ),
        (format!("/gateways/slack/{id}"), Some(secret.as_str())),
    ] {
        let refused = post_update(&api, &path, sent, &body).await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND, "{path} {sent:?}");
        assert_eq!(refused.error_message(), "Passerelle introuvable");
    }
    // With the secret, yet not what Telegram sends; or too large.
    let unreadable = post_update(&api, &path, Some(&secret), "pas du json").await;
    assert_eq!(unreadable.status, StatusCode::BAD_REQUEST);
    let large = format!(
        "{{\"update_id\": 2, \"pad\": \"{}\"}}",
        "a".repeat(300 * 1024)
    );
    let refused = post_update(&api, &path, Some(&secret), &large).await;
    assert_eq!(refused.status, StatusCode::PAYLOAD_TOO_LARGE);
    api.idle().await;
    assert!(telegram.messages().is_empty());

    // Read: Léa, linked to nothing, is told how to link, in her own chat.
    let taken = send_update(&api, &telegram, id, &body).await;
    assert_eq!(taken.status, StatusCode::OK, "{}", taken.text());
    // Sent again by Telegram: taken once.
    assert_eq!(
        send_update(&api, &telegram, id, &body).await.status,
        StatusCode::OK
    );
    let messages = telegram.messages();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0].bot, TELEGRAM_BOT);
    assert_eq!(messages[0].chat, LEA_TELEGRAM);
    assert!(
        messages[0].text.contains("« Mon compte »"),
        "{}",
        messages[0].text
    );
    assert!(
        !messages[0].text.contains("planning"),
        "{}",
        messages[0].text
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_but_a_person_s_own_message_in_their_private_chat_is_answered() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_telegram(&paul).await;
    let message = |update_id: i64, change: &dyn Fn(&mut Value)| {
        let mut sent: Value =
            serde_json::from_str(&update(update_id, LEA_TELEGRAM, LEA_USERNAME, "Bonjour"))
                .unwrap();
        change(&mut sent);
        sent.to_string()
    };
    let ignored = [
        // In a group, by a bot, without text, an edit, a button elsewhere.
        message(10, &|u| {
            u["message"]["chat"] = json!({ "id": -10_042, "type": "group" })
        }),
        message(11, &|u| u["message"]["from"]["is_bot"] = json!(true)),
        message(12, &|u| {
            u["message"].as_object_mut().unwrap().remove("text");
        }),
        message(13, &|u| {
            let edited = u["message"].take();
            u["edited_message"] = edited;
        }),
        json!({ "update_id": 14, "my_chat_member": { "chat": { "id": LEA_TELEGRAM } } })
            .to_string(),
    ];
    for body in &ignored {
        let taken = send_update(&api, &telegram, id, body).await;
        assert_eq!(taken.status, StatusCode::OK, "{body}");
    }
    // A paused bot answers, and does nothing.
    let paused = paul
        .patch(&format!("{GATEWAYS}/{id}"), json!({ "enabled": false }))
        .await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let taken = send_update(&api, &telegram, id, &message(15, &|_| {})).await;
    assert_eq!(taken.status, StatusCode::OK);
    assert!(telegram.messages().is_empty(), "{:?}", telegram.messages());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_bot_s_updates_go_without_the_header() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let id = Uuid::new_v4();
    // Reached: it answers as to an unknown gateway.
    let reached = post_update(&api, &format!("/gateways/telegram/{id}"), None, "{}").await;
    assert_eq!(reached.status, StatusCode::NOT_FOUND);
    for path in [
        format!("/gateways/telegram/{id}/"),
        format!("/gateways/telegram/{}", id.to_string().to_uppercase()),
        format!("/gateways/telegram/{}", id.simple()),
        "/gateways/telegram/pas-un-identifiant".to_owned(),
        format!("/gateways/telegram//{id}"),
        "/admin/gateways/telegram".to_owned(),
        format!("/account/gateways/{id}/telegram-code"),
        format!("/account/gateways/{id}/telegram-link"),
    ] {
        let refused = post_update(&api, &path, None, "{}").await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            refused.error_message(),
            "En-tête de requête manquant",
            "{path}"
        );
    }
}
