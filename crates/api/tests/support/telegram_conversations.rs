//! A person talking with the agent in Telegram in the tests (ADR-135):
//! their Telegram account linked to their account, what they write to the
//! bot as Telegram sends it, the conversations their private chat holds,
//! and the answers the pass writes back there; a press of a validation
//! button, as Telegram sends it, or as a client might claim it (ADR-136).

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeTelegram, TelegramMessage, update};
use mimorya_core::demo::tenant;
use serde_json::Value;
use uuid::Uuid;

use super::slack_conversations::settle;
use super::telegram::{ask_code, confirm, declare_telegram, open_and_accept, send_update};
use super::{Client, TestApi};

/// The bot, declared by `paul`, the Telegram user `from` (`@username`)
/// linked to `client`'s account with Telegram's updates 1 and 2.
pub async fn linked_bot(
    api: &TestApi,
    telegram: &FakeTelegram,
    (paul, client): (&Client<'_>, &Client<'_>),
    from: (i64, &str),
) -> Uuid {
    let id = declare_telegram(paul).await;
    link_to(api, telegram, (id, 1), client, from).await;
    id
}

/// Links the Telegram user `from` (`@username`) to `client`'s account on
/// the bot `id`, with Telegram's updates `update_id` and the next.
pub async fn link_to(
    api: &TestApi,
    telegram: &FakeTelegram,
    (id, update_id): (Uuid, i64),
    client: &Client<'_>,
    from: (i64, &str),
) {
    let code = ask_code(client, id).await;
    open_and_accept(api, telegram, id, update_id, from, &code).await;
    let confirmed = confirm(client, id).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.text());
}

/// Sends what `from` writes to the bot `id` as Telegram's update
/// `update_id`, then waits for what it started.
pub async fn talk(
    api: &TestApi,
    telegram: &FakeTelegram,
    id: Uuid,
    update_id: i64,
    (from, username): (i64, &str),
    text: &str,
) {
    let body = update(update_id, from, username, text);
    let sent = send_update(api, telegram, id, &body).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    settle(api).await;
}

/// What the bots wrote in the private chat of `person`, oldest first.
pub fn chat_of(telegram: &FakeTelegram, person: i64) -> Vec<TelegramMessage> {
    (telegram.messages().into_iter())
        .filter(|m| m.chat == person)
        .collect()
}

/// What the bots last wrote to `person`.
pub fn last_said(telegram: &FakeTelegram, person: i64) -> TelegramMessage {
    chat_of(telegram, person).pop().expect("a message")
}

/// The keyboard under the message numbered `number`, as it stands.
pub fn keyboard_of(telegram: &FakeTelegram, number: i64) -> Value {
    let message = &telegram.messages()[usize::try_from(number - 1).unwrap()];
    message.keyboard.clone().unwrap_or(Value::Null)
}

/// The number of the latest message the bot wrote.
pub fn latest_number(telegram: &FakeTelegram) -> i64 {
    i64::try_from(telegram.messages().len()).unwrap()
}

/// What the bot last answered a press.
pub fn last_brief(telegram: &FakeTelegram) -> String {
    telegram.answers().pop().expect("an answer").1
}

/// The conversation of the account `user_id` open in the private chat of
/// `person` with the bot `id`, if any.
pub async fn open_conversation(
    api: &TestApi,
    id: Uuid,
    person: i64,
    user_id: Uuid,
) -> Option<Uuid> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let open = sqlx::query_scalar(
        "select conversation_id from gateway_threads where gateway_id = $1 \
         and channel_id = $2 and user_id = $3 and closed_at is null",
    )
    .bind(id)
    .bind(person.to_string())
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    open
}

/// The runs started by writing to the bot, newest first: their
/// conversation and status (a bot declared again goes with the former
/// conversations' chats).
pub async fn telegram_runs(api: &TestApi) -> Vec<(Uuid, Uuid, String)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let runs = sqlx::query_as(
        "select r.id, r.conversation_id, r.status from agent_runs r \
         join gateway_threads t on t.conversation_id = r.conversation_id \
         where r.channel = 'telegram' order by r.created_at desc",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    runs
}

/// What became of the answers in the private chat of `person`: their
/// outcome (none while to try again) and attempts, oldest first.
pub async fn chat_replies(api: &TestApi, person: i64) -> Vec<(Option<String>, i32)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select r.outcome, r.attempts from gateway_replies r \
         join gateway_threads t on t.id = r.thread_id where t.channel_id = $1 \
         order by r.event_id",
    )
    .bind(person.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

/// Makes the answers in the private chat of `person` waiting to be tried
/// again due.
pub async fn retry_chat_now(api: &TestApi, person: i64) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update gateway_replies set retry_after = now() - interval '1 second' \
         where outcome is null and thread_id in \
         (select id from gateway_threads where channel_id = $1)",
    )
    .bind(person.to_string())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// What Telegram would send for the person `from` pressing, under the
/// message numbered `message_id` of their private chat, sent by `sent_by`
/// with a « Valider » and a « Refuser » button for `approval`, the one that
/// says `approved`: a press claiming buttons no message of the bot showed.
pub fn claimed_press(
    update_id: i64,
    (from, username): (i64, &str),
    (message_id, sent_by): (i64, i64),
    approval: Uuid,
    approved: bool,
) -> String {
    let data = if approved {
        format!("v:{approval}")
    } else {
        format!("r:{approval}")
    };
    serde_json::json!({
        "update_id": update_id,
        "callback_query": {
            "id": format!("rappel-{update_id}"),
            "from": { "id": from, "is_bot": false, "first_name": "Test", "username": username },
            "message": {
                "message_id": message_id,
                "from": { "id": sent_by, "is_bot": true, "first_name": "Assistant" },
                "chat": { "id": from, "type": "private" },
                "date": 1_700_000_000,
                "text": "Une action attend",
                "reply_markup": { "inline_keyboard": [[
                    { "text": "Valider", "callback_data": format!("v:{approval}") },
                    { "text": "Refuser", "callback_data": format!("r:{approval}") }
                ]] }
            },
            "chat_instance": "-1",
            "data": data
        }
    })
    .to_string()
}

/// Sends what Telegram sends, then waits for what it started.
pub async fn deliver(api: &TestApi, telegram: &FakeTelegram, id: Uuid, body: &str) {
    let sent = send_update(api, telegram, id, body).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    settle(api).await;
}
