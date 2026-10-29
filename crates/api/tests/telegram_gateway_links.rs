//! A person links their Telegram account (ADR-133): Telegram proves no
//! address, so they ask for a code in their own session, open the bot's
//! link carrying it in Telegram, which sends it back from their private
//! chat; the bot names whose account it links, they accept it there, and
//! confirm in their session whom it came from. A code links nothing without
//! the session that asked for it, nor once taken, forgotten or expired;
//! whoever else accepts it first only proposes a link the session sees and
//! may refuse. Talking with the agent, and an enabled bot of the company's
//! own, are required. Each step is audited, never a code.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_connectors::testing::{FakeSlack, FakeTelegram};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::gateways::{GATEWAYS, declare_slack};
use support::manager_approvals::without;
use support::refusals::{entries_since, latest_entry};
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{
    LEA_TELEGRAM, LEA_USERNAME, MARC_TELEGRAM, MARC_USERNAME, ask_code, ask_link, code_of, confirm,
    declare_telegram, of_person, open_and_accept, other_company_bot, write,
};

const LEA: (i64, &str) = (LEA_TELEGRAM, LEA_USERNAME);
const MARC: (i64, &str) = (MARC_TELEGRAM, MARC_USERNAME);

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_links_their_telegram_account_from_their_own_session() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let id = declare_telegram(&paul).await;
    let since = latest_entry(&api).await;

    // Asked for in her session: the bot's link, pending until the time it
    // was given, which the page matches its link with.
    let asked = ask_link(&lea, id).await;
    let code = code_of(&asked);
    let pending = &of_person(&lea, id).await["pending"];
    assert!(pending["expiresAt"].is_string(), "{pending}");
    assert_eq!(pending["expiresAt"], asked["expiresAt"]);
    assert_eq!(pending["claimedBy"], Value::Null);
    // Opened in Telegram: the bot names whose account it links.
    let offered = write(&api, &telegram, id, 1, LEA, &format!("/start {code}")).await;
    assert!(offered.contains("au compte de Léa Fontaine"), "{offered}");
    assert_eq!(
        of_person(&lea, id).await["pending"]["claimedBy"],
        Value::Null
    );
    // Accepted there: her session sees whom from.
    let told = write(&api, &telegram, id, 2, LEA, "/oui").await;
    assert!(told.starts_with("C’est noté"), "{told}");
    let view = of_person(&lea, id).await;
    assert_eq!(view["pending"]["claimedBy"], format!("@{LEA_USERNAME}"));
    assert_eq!(view["pending"]["claimedId"], LEA_TELEGRAM.to_string());
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
    let again = write(&api, &telegram, id, 3, LEA, &format!("/start {code}")).await;
    assert!(again.contains("n’est plus valable"), "{again}");
    assert!(
        write(&api, &telegram, id, 4, LEA, "/oui")
            .await
            .starts_with("Aucune liaison"),
    );
    // Writing now, she is told to whose account hers is linked.
    let told = write(&api, &telegram, id, 5, LEA, "Bonjour").await;
    assert!(
        told.starts_with("Votre compte Telegram est relié au compte de Léa Fontaine"),
        "{told}"
    );

    // Audited: to whom it was offered, who accepted it, the link; never
    // the code.
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.link_code_offered"), "{audited}");
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
    // Marc accepts Léa's code first, told it links Léa's account: only
    // proposed to her session.
    open_and_accept(&api, &telegram, id, 1, MARC, &code).await;
    // Léa's own, after: the code was accepted already.
    let told = write(&api, &telegram, id, 3, LEA, &format!("/start {code}")).await;
    assert!(told.contains("n’est plus valable"), "{told}");
    // Marc's session has nothing to confirm; Léa's sees whom from.
    assert_eq!(confirm(&marc, id).await.status, StatusCode::BAD_REQUEST);
    let pending = &of_person(&lea, id).await["pending"];
    assert_eq!(pending["claimedBy"], format!("@{MARC_USERNAME}"));
    assert_eq!(pending["claimedId"], MARC_TELEGRAM.to_string());
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
    let late = write(&api, &telegram, id, 4, LEA, &format!("/start {code}")).await;
    assert!(late.contains("n’est plus valable"), "{late}");
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
    let (_, theirs, _) = other_company_bot(&api, "acme-telegram-links").await;
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
    open_and_accept(&api, &telegram, id, 1, LEA, &code).await;
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
