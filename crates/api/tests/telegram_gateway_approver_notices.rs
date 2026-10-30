//! Requests awaiting a manager told in Telegram (ADR-137): the manager
//! linked to the company's bot is written, in their private chat with it, a
//! paused run's requests awaiting them, once, the actions that may be
//! validated there with their exact parameters and buttons; pressed, the
//! request is decided as that manager, audited so, the press answered, its
//! buttons taken off and the outcome said in answer. While the manager is
//! away, whom they named to stand in for them is told instead, and decides.
//! An action sent outside the company is told without buttons, and no press
//! decides it; nor does another linked person's press. A manager who lost
//! the right to decide is written nothing.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::gateways::APPROVAL_NOTIFIED;
use mimorya_connectors::testing::{FakeTelegram, TELEGRAM_BOT};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::approver_notices::{nadia_s_request, notice_of, notify, refuse};
use support::delegations::{away, back, julien_decides_for_the_south};
use support::manager_approvals::{listed, without};
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{LEA_TELEGRAM, LEA_USERNAME, MARC_TELEGRAM, MARC_USERNAME};
use support::telegram_conversations::{
    chat_of, claimed_press, deliver, keyboard_of, last_brief, last_said, latest_number, link_to,
    linked_bot,
};
use uuid::Uuid;

const MARC: (i64, &str) = (MARC_TELEGRAM, MARC_USERNAME);
const LEA: (i64, &str) = (LEA_TELEGRAM, LEA_USERNAME);
/// Julien's Telegram account.
const JULIEN_TELEGRAM: i64 = 7171;
const JULIEN: (i64, &str) = (JULIEN_TELEGRAM, "julien_mo");

/// The metadata of each notice audited for the request.
async fn audited(api: &TestApi, approval: Uuid) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_scalar(
        "select metadata from audit_log where action = $1 and resource_id = $2 order by id",
    )
    .bind(APPROVAL_NOTIFIED)
    .bind(approval.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_manager_is_told_in_telegram_and_validates_with_a_button() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;

    // Told once, in his private chat, with the action's exact parameters
    // and its buttons.
    let before = chat_of(&telegram, MARC_TELEGRAM).len();
    assert_eq!(notify(&api).await, 1);
    assert_eq!(notify(&api).await, 0);
    assert_eq!(chat_of(&telegram, MARC_TELEGRAM).len(), before + 1);
    let notice = last_said(&telegram, MARC_TELEGRAM);
    let number = latest_number(&telegram);
    assert!(notice.html);
    assert!(
        notice
            .text
            .starts_with("<b>Nadia Benali</b> : l’agent attend votre validation pour une action."),
        "{}",
        notice.text
    );
    assert!(notice.text.contains("/approvals\">"), "{}", notice.text);
    assert!(notice.text.contains("<pre>"), "{}", notice.text);
    assert!(notice.text.contains("proposalId"), "{}", notice.text);
    let keyboard = keyboard_of(&telegram, number);
    assert_eq!(
        keyboard["inline_keyboard"][0][0]["callback_data"],
        format!("v:{approval}")
    );
    assert_eq!(
        notice_of(&api, approval).await,
        Some((Some("sent".into()), 1))
    );
    let manager = json!({
        "gatewayId": id,
        "gateway": "telegram",
        "approverId": user::MARC,
        "grouped": 1,
    });
    assert_eq!(audited(&api, approval).await, [manager]);

    // Validated in Telegram: decided as him, audited so, the press
    // answered, its buttons taken off, the outcome said in answer.
    let since = latest_entry(&api).await;
    let press = telegram.press(10, MARC, number, &format!("v:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Validée");
    let said = last_said(&telegram, MARC_TELEGRAM);
    assert_eq!(said.reply_to, Some(number));
    assert_eq!(
        said.text,
        "Validée par vous dans Telegram. La demande reprend."
    );
    assert_eq!(
        keyboard_of(&telegram, number),
        json!({ "inline_keyboard": [] })
    );
    let decided = listed(&marc, &run).await.expect("the request he decided");
    assert_eq!(decided["status"], "approved");
    assert_eq!(decided["decidedBy"]["id"], json!(user::MARC));
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("approval.approved"), "{audited}");
    assert!(audited.contains(r#""gateway":"telegram""#), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_stand_in_is_told_and_decides_in_the_manager_s_absence() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let julien = api.sign_in(user::JULIEN).await;
    let assignment = julien_decides_for_the_south(&paul).await;
    away(&marc, user::JULIEN, 0).await;
    let id = linked_bot(&api, &telegram, (&paul, &julien), JULIEN).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;

    assert_eq!(notify(&api).await, 1);
    let notice = last_said(&telegram, JULIEN_TELEGRAM);
    assert!(
        notice.text.contains(", en l’absence de Marc Lefèvre."),
        "{}",
        notice.text
    );
    let audit = json!({
        "gatewayId": id,
        "gateway": "telegram",
        "approverId": user::MARC,
        "grouped": 1,
        "delegateId": user::JULIEN,
    });
    assert_eq!(audited(&api, approval).await, [audit]);
    let number = latest_number(&telegram);
    let press = telegram.press(10, JULIEN, number, &format!("r:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Refusée");
    let decided = listed(&julien, &run).await.expect("the request he decided");
    assert_eq!(decided["status"], "rejected");
    assert_eq!(decided["decidedBy"]["id"], json!(user::JULIEN));

    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_sent_outside_the_company_is_told_without_buttons() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (run, approval) = nadia_s_request(&api, &paul, "09:00").await;
    // As an e-mail sent in her name would be.
    as_owner(
        "update approvals set risk = 'external' where id = $1",
        approval,
    )
    .await;

    assert_eq!(notify(&api).await, 1);
    let notice = last_said(&telegram, MARC_TELEGRAM);
    let number = latest_number(&telegram);
    assert_eq!(notice.keyboard, None);
    assert!(
        notice
            .text
            .contains("Elle se valide dans l’application, qui en montre les paramètres exacts"),
        "{}",
        notice.text
    );
    assert!(!notice.text.contains("<pre>"), "{}", notice.text);

    // A press claiming its buttons under the bot's message decides nothing.
    let claimed = claimed_press(10, MARC, (number, TELEGRAM_BOT), approval, true);
    deliver(&api, &telegram, id, &claimed).await;
    let said = last_said(&telegram, MARC_TELEGRAM).text;
    assert!(
        said.starts_with("Cette action se valide dans l’application"),
        "{said}"
    );
    // Nor does another linked person's.
    let lea = api.sign_in(user::LEA).await;
    link_to(&api, &telegram, (id, 11), &lea, LEA).await;
    let claimed = claimed_press(13, LEA, (number, TELEGRAM_BOT), approval, true);
    deliver(&api, &telegram, id, &claimed).await;
    let said = last_said(&telegram, LEA_TELEGRAM).text;
    assert_eq!(said, "Demande de validation introuvable");
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
    refuse(&api, &marc, approval).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_who_lost_the_right_is_written_nothing() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (_, approval) = nadia_s_request(&api, &paul, "09:00").await;

    // Named when the request was made, he loses the right before its
    // notice: neither its parameters nor its buttons are written to him.
    let before = chat_of(&telegram, MARC_TELEGRAM).len();
    let policy = without(&paul, user::MARC, "approval:decide").await;
    assert_eq!(notify(&api).await, 0);
    assert_eq!(chat_of(&telegram, MARC_TELEGRAM).len(), before);
    assert_eq!(
        notice_of(&api, approval).await,
        Some((Some("skipped".into()), 1))
    );
    assert!(audited(&api, approval).await.is_empty());
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    refuse(&api, &marc, approval).await;
}
