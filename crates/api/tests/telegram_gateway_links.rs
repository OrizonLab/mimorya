//! A person links their Telegram account (ADR-133): Telegram proves no
//! address, so they ask for a code in their own session, open the bot's
//! link carrying it in Telegram, which sends it back from their private
//! chat, and confirm in their session whom it came from. A code links
//! nothing without the session that asked for it, nor once taken, forgotten
//! or expired; whoever else sends it back first only proposes a link the
//! session sees and may refuse. Talking with the agent, and an enabled bot
//! of the company's own, are required. Each step is audited, never a code.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_connectors::testing::{FakeSlack, FakeTelegram, update};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::gateways::{GATEWAYS, declare_slack};
use support::manager_approvals::without;
use support::refusals::{entries_since, latest_entry};
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{
    LEA_TELEGRAM, LEA_USERNAME, MARC_TELEGRAM, MARC_USERNAME, ask_code, confirm, declare_telegram,
    of_person, other_company_bot, send_update,
};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_links_their_telegram_account_from_their_own_session() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let id = declare_telegram(&paul).await;
    let since = latest_entry(&api).await;

    // Asked for in her session: the bot's link, pending.
    let code = ask_code(&lea, id).await;
    let pending = &of_person(&lea, id).await["pending"];
    assert!(pending["expiresAt"].is_string(), "{pending}");
    assert_eq!(pending["claimedBy"], Value::Null);
    // Opened in Telegram: sent back from her private chat.
    let start = update(1, LEA_TELEGRAM, LEA_USERNAME, &format!("/start {code}"));
    assert_eq!(
        send_update(&api, &telegram, id, &start).await.status,
        StatusCode::OK
    );
    let told = telegram.messages().pop().unwrap();
    assert_eq!(told.chat, LEA_TELEGRAM);
    assert!(told.text.starts_with("C’est bien reçu"), "{}", told.text);
    let view = of_person(&lea, id).await;
    assert_eq!(view["pending"]["claimedBy"], format!("@{LEA_USERNAME}"));
    assert_eq!(view["linked"], Value::Null);

    // Confirmed in her session: linked, nothing pending.
    let confirmed = confirm(&lea, id).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.text());
    let view = of_person(&lea, id).await;
    assert_eq!(view["pending"], Value::Null);
    let linked = &view["linked"];
    assert_eq!(linked["externalId"], LEA_TELEGRAM.to_string());
    assert_eq!(linked["name"], format!("@{LEA_USERNAME}"));
    assert_eq!(linked["address"], Value::Null);
    // The code is taken: sent again, it links nothing.
    let again = update(2, LEA_TELEGRAM, LEA_USERNAME, &format!("/start {code}"));
    send_update(&api, &telegram, id, &again).await;
    let told = telegram.messages().pop().unwrap();
    assert!(told.text.contains("n’est plus valable"), "{}", told.text);
    // Writing now, she is told the conversation here is to come.
    let hello = update(3, LEA_TELEGRAM, LEA_USERNAME, "Bonjour");
    send_update(&api, &telegram, id, &hello).await;
    let told = telegram.messages().pop().unwrap();
    assert!(
        told.text.starts_with("Votre compte Telegram est relié"),
        "{}",
        told.text
    );

    // Audited: whom the code came back from, the link; never the code.
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.link_code_claimed"), "{audited}");
    assert!(audited.contains("gateway.identity_linked"), "{audited}");
    assert!(!audited.contains(&code), "{audited}");
    // Removed by her.
    let removed = lea.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(of_person(&lea, id).await["linked"], Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_code_links_nothing_without_the_session_that_asked_for_it() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let marc = api.sign_in(user::MARC).await;
    let id = declare_telegram(&paul).await;

    // Nothing to confirm before it comes back.
    let code = ask_code(&lea, id).await;
    let early = confirm(&lea, id).await;
    assert_eq!(early.status, StatusCode::BAD_REQUEST, "{}", early.text());
    // Marc sends Léa's code back first: only proposed to her session.
    let stolen = update(1, MARC_TELEGRAM, MARC_USERNAME, &format!("/start {code}"));
    send_update(&api, &telegram, id, &stolen).await;
    // Léa's own, after: the code was sent back already.
    let hers = update(2, LEA_TELEGRAM, LEA_USERNAME, &format!("/start {code}"));
    send_update(&api, &telegram, id, &hers).await;
    let told = telegram.messages().pop().unwrap();
    assert_eq!(told.chat, LEA_TELEGRAM);
    assert!(told.text.contains("n’est plus valable"), "{}", told.text);
    // Marc's session has nothing to confirm; Léa's sees whom from.
    assert_eq!(confirm(&marc, id).await.status, StatusCode::BAD_REQUEST);
    let pending = &of_person(&lea, id).await["pending"];
    assert_eq!(pending["claimedBy"], format!("@{MARC_USERNAME}"));
    assert_eq!(of_person(&marc, id).await["pending"], Value::Null);
    // She forgets it: nothing to confirm, the code links nothing.
    let path = format!("/account/gateways/{id}/telegram-code");
    assert_eq!(lea.delete(&path).await.status, StatusCode::OK);
    assert_eq!(of_person(&lea, id).await["pending"], Value::Null);
    assert_eq!(confirm(&lea, id).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(lea.delete(&path).await.status, StatusCode::NOT_FOUND);

    // A code past its time links nothing, nor its claim.
    let code = ask_code(&lea, id).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update gateway_link_codes set expires_at = $1")
        .bind(Utc::now() - Duration::seconds(1))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(of_person(&lea, id).await["pending"], Value::Null);
    let late = update(3, LEA_TELEGRAM, LEA_USERNAME, &format!("/start {code}"));
    send_update(&api, &telegram, id, &late).await;
    let told = telegram.messages().pop().unwrap();
    assert!(told.text.contains("n’est plus valable"), "{}", told.text);
    assert_eq!(confirm(&lea, id).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(of_person(&lea, id).await["linked"], Value::Null);
    assert_eq!(of_person(&marc, id).await["linked"], Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_an_enabled_bot_of_one_s_company_links_who_may_talk_with_the_agent() {
    let (slack, telegram) = (FakeSlack::start().await, FakeTelegram::start().await);
    let api = TestApi::start_with_gateways(&SSO_DEPLOYMENT, &slack.url, &telegram.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let ask = |id: uuid::Uuid| format!("/account/gateways/{id}/telegram-code");

    // Another company's bot, or a Slack gateway: as absent.
    let (_, theirs) = other_company_bot(&api, "acme-telegram-links").await;
    assert_eq!(
        lea.post(&ask(theirs), json!({})).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(confirm(&lea, theirs).await.status, StatusCode::NOT_FOUND);
    let slack = declare_slack(&paul).await;
    assert_eq!(
        lea.post(&ask(slack), json!({})).await.status,
        StatusCode::NOT_FOUND
    );

    // Paused: nothing is linked.
    let id = declare_telegram(&paul).await;
    let code = ask_code(&lea, id).await;
    let start = update(1, LEA_TELEGRAM, LEA_USERNAME, &format!("/start {code}"));
    send_update(&api, &telegram, id, &start).await;
    let paused = paul
        .patch(&format!("{GATEWAYS}/{id}"), json!({ "enabled": false }))
        .await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    assert_eq!(confirm(&lea, id).await.status, StatusCode::CONFLICT);
    assert_eq!(
        lea.post(&ask(id), json!({})).await.status,
        StatusCode::CONFLICT
    );
    assert_eq!(of_person(&lea, id).await["linked"], Value::Null);

    // Who may not talk with the agent links nothing.
    let enabled = paul
        .patch(&format!("{GATEWAYS}/{id}"), json!({ "enabled": true }))
        .await;
    assert_eq!(enabled.status, StatusCode::OK);
    let policy = without(&paul, user::LEA, "agent:use").await;
    assert_eq!(of_person(&lea, id).await["available"], false);
    assert_eq!(
        lea.post(&ask(id), json!({})).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(confirm(&lea, id).await.status, StatusCode::FORBIDDEN);
    assert_eq!(of_person(&lea, id).await["linked"], Value::Null);
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
}
