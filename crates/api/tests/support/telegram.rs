//! The company's Telegram bot in the tests (ADR-133): declared by an
//! administrator with the local Telegram's bot, updates sent as Telegram
//! sends them, with the secret the platform gave it, and a person's link
//! code asked for in their session, opened and accepted in Telegram.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mimorya_api::services::AccessContext;
use mimorya_api::services::admin::{NewTelegramGatewayInput, declare_telegram_gateway};
use mimorya_connectors::testing::{
    FakeTelegram, OTHER_BOT_TOKEN, TELEGRAM_BOT, TELEGRAM_TOKEN, update,
};
use serde_json::{Value, json};
use uuid::Uuid;

use super::gateways::{GATEWAYS, remove_gateways};
use super::slack_companies::other_administrator;
use super::{Client, TestApi, TestResponse};

pub const DECLARE_TELEGRAM: &str = "/admin/gateways/telegram";
/// Léa's Telegram account, and how Telegram shows her.
pub const LEA_TELEGRAM: i64 = 4242;
pub const LEA_USERNAME: &str = "lea_font";
/// Marc's Telegram account.
pub const MARC_TELEGRAM: i64 = 5151;
pub const MARC_USERNAME: &str = "marc_lefevre";

/// The company's Telegram bot, declared by `admin` with the local
/// Telegram's bot; the gateways a previous test declared go first.
pub async fn declare_telegram(admin: &Client<'_>) -> Uuid {
    remove_gateways(admin).await;
    let declared = admin
        .post(
            DECLARE_TELEGRAM,
            json!({ "name": "Telegram Néo", "botToken": TELEGRAM_TOKEN }),
        )
        .await;
    assert_eq!(declared.status, StatusCode::CREATED, "{}", declared.text());
    declared.json()["id"].as_str().unwrap().parse().unwrap()
}

/// Posts `body` to the bot `id`'s address as Telegram would, with
/// `secret` in its header if any, without the browser's header.
pub async fn post_update(
    api: &TestApi,
    path: &str,
    secret: Option<&str>,
    body: &str,
) -> TestResponse {
    let mut request = Request::post(path).header("content-type", "application/json");
    if let Some(secret) = secret {
        request = request.header("x-telegram-bot-api-secret-token", secret);
    }
    api.send(request.body(Body::from(body.to_owned())).unwrap())
        .await
}

/// What Telegram sending `body` to the bot `id`, with the secret the
/// platform gave it, gets; once answered, what the bot wrote is written.
pub async fn send_update(
    api: &TestApi,
    telegram: &FakeTelegram,
    id: Uuid,
    body: &str,
) -> TestResponse {
    let (_, secret) = telegram.webhook(TELEGRAM_BOT).expect("a webhook");
    let sent = post_update(
        api,
        &format!("/gateways/telegram/{id}"),
        Some(&secret),
        body,
    )
    .await;
    api.idle().await;
    sent
}

/// What the bot `id` answers the Telegram user `from` (`@username`)
/// writing `text`, sent by Telegram as its update `update_id`.
pub async fn write(
    api: &TestApi,
    telegram: &FakeTelegram,
    id: Uuid,
    update_id: i64,
    (from, username): (i64, &str),
    text: &str,
) -> String {
    let body = update(update_id, from, username, text);
    let sent = send_update(api, telegram, id, &body).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    let answer = telegram.messages().pop().expect("an answer");
    assert_eq!(answer.chat, from, "{}", answer.text);
    answer.text
}

/// The Telegram user `from` opens the link of `code` and accepts it, as
/// Telegram's updates `update_id` and the next one: what the bot answered
/// last.
pub async fn open_and_accept(
    api: &TestApi,
    telegram: &FakeTelegram,
    id: Uuid,
    update_id: i64,
    from: (i64, &str),
    code: &str,
) -> String {
    let start = format!("/start {code}");
    let offered = write(api, telegram, id, update_id, from, &start).await;
    assert!(offered.contains("envoyez /oui"), "{offered}");
    write(api, telegram, id, update_id + 1, from, "/oui").await
}

/// The gateway `id` as `person` lists it in their account.
pub async fn of_person(person: &Client<'_>, id: Uuid) -> Value {
    let listed = person.get("/account/gateways").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    (listed.json()["gateways"].as_array().unwrap().iter())
        .find(|g| g["gatewayId"] == id.to_string())
        .cloned()
        .unwrap_or(Value::Null)
}

/// A new code for `person` on the bot `id`: the code its link carries.
pub async fn ask_code(person: &Client<'_>, id: Uuid) -> String {
    let asked = person
        .post(&format!("/account/gateways/{id}/telegram-code"), json!({}))
        .await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.text());
    let link = asked.json()["link"].as_str().unwrap().to_owned();
    let code = link
        .strip_prefix("https://t.me/NeoMaintenanceBot?start=")
        .unwrap_or_else(|| panic!("{link}"));
    code.to_owned()
}

/// What confirming the link `person` asked for on the bot `id` gets.
pub async fn confirm(person: &Client<'_>, id: Uuid) -> TestResponse {
    person
        .post(&format!("/account/gateways/{id}/telegram-link"), json!({}))
        .await
}

/// The gateway's view in the administration's listing.
pub async fn listed_gateway(admin: &Client<'_>, id: Uuid) -> Value {
    let listed = admin.get(GATEWAYS).await.json();
    (listed["gateways"].as_array().unwrap().iter())
        .find(|g| g["id"] == id.to_string())
        .cloned()
        .unwrap_or(Value::Null)
}

/// Another company, its own Telegram bot, declared by its administrator
/// with the local Telegram's other bot, and that administrator.
pub async fn other_company_bot(api: &TestApi, slug: &str) -> (Uuid, Uuid, AccessContext) {
    let (tenant_id, jeanne) = other_administrator(api, slug).await;
    let input = NewTelegramGatewayInput {
        name: "Telegram Acme".into(),
        bot_token: OTHER_BOT_TOKEN.into(),
        enabled: true,
    };
    let gateway = declare_telegram_gateway(&api.app.services.gateways, &jeanne, input)
        .await
        .unwrap();
    (tenant_id, gateway, jeanne)
}
