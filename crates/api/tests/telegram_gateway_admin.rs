//! The company's Telegram bot, declared by its connector administrators
//! (ADR-133): its token checked with Telegram before anything is kept, the
//! bot it is kept, Telegram told to send its updates to the gateway's
//! address with a secret the platform made, a refusal keeping nothing; its
//! token and secret sealed, never returned nor audited; one per company; a
//! new token only of the same bot; deleted with its webhook. Reserved to
//! the connectors' administrators.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{
    FakeTelegram, OTHER_BOT_TOKEN, TELEGRAM_BOT, TELEGRAM_BOT_USERNAME, TELEGRAM_TOKEN,
};
use mimorya_core::demo::user;
use mimorya_core::gateways::telegram::is_webhook_secret;
use serde_json::json;
use support::gateways::{GATEWAYS, remove_gateways};
use support::refusals::{entries_since, latest_entry};
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{DECLARE_TELEGRAM, declare_telegram, listed_gateway};
use support::{TestApi, WEB_ORIGIN};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_declares_its_telegram_bot_once_checked_with_telegram() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let paul = api.sign_in(user::PAUL).await;
    remove_gateways(&paul).await;
    let since = latest_entry(&api).await;
    let declare = |token: &str| json!({ "name": "Telegram", "botToken": token });

    // Written as BotFather gives them, or refused before Telegram is asked.
    let refused = paul
        .post(DECLARE_TELEGRAM, declare("xoxb-pas-un-bot"))
        .await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused.error_message().contains("BotFather"),
        "{}",
        refused.text()
    );
    // A token Telegram refuses.
    let unknown = "111111111:AAH-fictif_jeton_inconnu_de_telegram_01";
    let refused = paul.post(DECLARE_TELEGRAM, declare(unknown)).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused
            .error_message()
            .starts_with("Telegram refuse le jeton du bot"),
        "{}",
        refused.text()
    );
    // An address Telegram does not take: nothing is kept.
    telegram.refuse_webhooks(true);
    let refused = paul.post(DECLARE_TELEGRAM, declare(TELEGRAM_TOKEN)).await;
    telegram.refuse_webhooks(false);
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused.error_message().contains("HTTPS"),
        "{}",
        refused.text()
    );
    assert_eq!(paul.get(GATEWAYS).await.json()["gateways"], json!([]));

    let id = declare_telegram(&paul).await;
    let view = listed_gateway(&paul, id).await;
    assert_eq!(view["type"], "telegram");
    assert_eq!(view["name"], "Telegram Néo");
    assert_eq!(view["enabled"], true);
    assert_eq!(view["workspaceId"], TELEGRAM_BOT.to_string());
    assert_eq!(view["workspaceName"], TELEGRAM_BOT_USERNAME);
    assert_eq!(view["botUserId"], TELEGRAM_BOT.to_string());
    assert_eq!(view["secretsStored"], true);
    assert_eq!(view["missingScopes"], serde_json::Value::Null);
    let updates_url = format!("{WEB_ORIGIN}/api/gateways/telegram/{id}");
    assert_eq!(view["eventsUrl"], updates_url);
    // Telegram told where, with a secret the platform made.
    let (url, secret) = telegram.webhook(TELEGRAM_BOT).expect("a webhook");
    assert_eq!(url, updates_url);
    assert!(is_webhook_secret(&secret), "{secret}");
    let listing = paul.get(GATEWAYS).await.text();
    assert!(!listing.contains(TELEGRAM_TOKEN) && !listing.contains(&secret));

    // One per company: Telegram is not told anything else.
    let again = paul.post(DECLARE_TELEGRAM, declare(TELEGRAM_TOKEN)).await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.text());
    assert_eq!(telegram.webhook(TELEGRAM_BOT), Some((url, secret.clone())));

    // Audited, its token and secret never.
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.created"), "{audited}");
    assert!(audited.contains(TELEGRAM_BOT_USERNAME), "{audited}");
    assert!(!audited.contains(TELEGRAM_TOKEN) && !audited.contains(&secret));
    assert!(!audited.contains(unknown));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_bot_s_token_changes_for_the_same_bot_only_and_goes_with_its_webhook() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_telegram(&paul).await;
    let path = format!("{GATEWAYS}/{id}");
    let (_, secret) = telegram.webhook(TELEGRAM_BOT).unwrap();
    let since = latest_entry(&api).await;

    let changed = paul
        .patch(
            &path,
            json!({ "name": "Bot de l’atelier", "enabled": false }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let view = listed_gateway(&paul, id).await;
    assert_eq!(view["name"], "Bot de l’atelier");
    assert_eq!(view["enabled"], false);

    // Another bot's token: the identities are this bot's users'.
    let other = paul
        .patch(&path, json!({ "botToken": OTHER_BOT_TOKEN }))
        .await;
    assert_eq!(other.status, StatusCode::BAD_REQUEST, "{}", other.text());
    assert!(
        other.error_message().contains("autre bot"),
        "{}",
        other.text()
    );
    // A bot has no signing secret.
    let signing = paul
        .patch(
            &path,
            json!({ "signingSecret": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6" }),
        )
        .await;
    assert_eq!(
        signing.status,
        StatusCode::BAD_REQUEST,
        "{}",
        signing.text()
    );
    // The same bot's: its webhook given again, the secret kept.
    telegram.unavailable(true);
    let down = paul
        .patch(&path, json!({ "botToken": TELEGRAM_TOKEN }))
        .await;
    telegram.unavailable(false);
    assert_eq!(down.status, StatusCode::BAD_REQUEST, "{}", down.text());
    let renewed = paul
        .patch(
            &path,
            json!({ "botToken": TELEGRAM_TOKEN, "enabled": true }),
        )
        .await;
    assert_eq!(renewed.status, StatusCode::OK, "{}", renewed.text());
    assert_eq!(telegram.webhook(TELEGRAM_BOT).unwrap().1, secret);
    // Nothing to check with Telegram as with Slack.
    let checked = paul.post(&format!("{path}/check"), json!({})).await;
    assert_eq!(
        checked.status,
        StatusCode::BAD_REQUEST,
        "{}",
        checked.text()
    );
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("\"botTokenChanged\":true"), "{audited}");
    assert!(!audited.contains(TELEGRAM_TOKEN) && !audited.contains(&secret));

    // Deleted: Telegram sends its updates nowhere, nothing stays.
    let deleted = paul.delete(&path).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    assert_eq!(telegram.webhook(TELEGRAM_BOT), None);
    assert_eq!(paul.get(GATEWAYS).await.json()["gateways"], json!([]));
    let mut tx = api
        .app
        .services
        .db
        .begin(mimorya_core::demo::tenant::ID)
        .await
        .unwrap();
    let secrets: i64 =
        sqlx::query_scalar("select count(*) from secrets where name like 'gateway.%'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(secrets, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_connectors_administrators_declare_a_bot() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let paul = api.sign_in(user::PAUL).await;
    remove_gateways(&paul).await;
    let lea = api.sign_in(user::LEA).await;
    let declared = lea
        .post(
            DECLARE_TELEGRAM,
            json!({ "name": "Telegram", "botToken": TELEGRAM_TOKEN }),
        )
        .await;
    assert_eq!(declared.status, StatusCode::FORBIDDEN);
    assert_eq!(telegram.webhook(TELEGRAM_BOT), None);
}
