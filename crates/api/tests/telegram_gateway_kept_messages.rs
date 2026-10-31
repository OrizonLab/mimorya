//! The Telegram messages kept to be written again once their actions are
//! settled (ADR-148): a pause's or a notice's, written with buttons, is kept
//! with the requests it shows. Of two actions, the one decided is said as
//! settled, without its buttons, the other still to validate, until it is
//! decided too; an expired one is said to await nothing more. One written
//! before it was kept, decided, only has its buttons taken off, its text
//! left as it was; its HTML refused, it is written again in plain text. A notice's message is
//! forgotten once its requests are past, or once it is no longer the
//! private chat of the Telegram account linked to its manager (unlinked,
//! or another linked); a pause's never by that pass.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::FakeTelegram;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::gateway_cards::purge_notice_cards;
use serde_json::json;
use support::TestApi;
use support::approver_notices::{nadia_s_request, notify, refuse};
use support::manager_approvals::listed;
use support::planning::{apply_proposal_times, propose};
use support::queue::as_owner;
use support::slack_conversations::answer;
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{MARC_TELEGRAM, MARC_USERNAME};
use support::telegram_conversations::{
    deliver, keyboard_of, last_brief, latest_number, link_to, linked_bot, message_numbered, talk,
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
async fn a_message_of_two_actions_says_each_as_it_stands() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal_times("", 2),
        ScriptedStep::text("Appliqué."),
    ]);
    talk(&api, &telegram, id, 10, MARC, "Planifie INT-2052").await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    let pause = message_numbered(&telegram, number).text;
    assert!(pause.contains("<b>Action 1 à valider :</b>"), "{pause}");
    assert!(pause.contains("<b>Action 2 à valider :</b>"), "{pause}");
    let data = |row: usize| {
        let keyboard = keyboard_of(&telegram, number);
        keyboard["inline_keyboard"][row][0]["callback_data"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let (first, second) = (data(0), data(1));

    // The first validated: said so, its buttons off, the other still open.
    deliver(
        &api,
        &telegram,
        id,
        &telegram.press(11, MARC, number, &first),
    )
    .await;
    assert_eq!(last_brief(&telegram), "Validée");
    let once = message_numbered(&telegram, number);
    assert_eq!(once.edits, 1);
    assert!(
        once.text
            .starts_with("Des actions de cette demande attendent votre validation"),
        "{}",
        once.text
    );
    assert!(once.text.contains("<b>Action 1 :</b>"), "{}", once.text);
    assert!(
        once.text
            .contains("</pre>\n<i>Validée.</i>\n\n<b>Action 2 à valider :</b>")
    );
    let rows = &keyboard_of(&telegram, number)["inline_keyboard"];
    assert_eq!(rows.as_array().map(Vec::len), Some(1));
    assert_eq!(rows[0][0]["callback_data"], second);
    assert_eq!(rows[0][0]["text"], "Valider l’action 2");

    // The second refused: none awaits any more, both said as they stand.
    let refuse = second.replacen("v:", "r:", 1);
    deliver(
        &api,
        &telegram,
        id,
        &telegram.press(12, MARC, number, &refuse),
    )
    .await;
    assert_eq!(last_brief(&telegram), "Refusée");
    let twice = message_numbered(&telegram, number);
    assert_eq!(twice.edits, 2);
    assert!(
        twice.text.starts_with(
            "Les actions de cette demande montrées ci-dessous sont décidées (<a href="
        ),
        "{}",
        twice.text
    );
    assert!(twice.text.contains("<i>Validée.</i>"), "{}", twice.text);
    assert!(twice.text.ends_with("<i>Refusée.</i>"), "{}", twice.text);
    assert!(!twice.text.contains("à valider"), "{}", twice.text);
    assert_eq!(
        keyboard_of(&telegram, number),
        json!({ "inline_keyboard": [] })
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_expired_action_s_message_says_it_awaits_nothing_more() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (run, approval) = asked(&api, &telegram, (id, MARC), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    as_owner(
        "update approvals set expires_at = now() - interval '1 minute' where id = $1",
        approval,
    )
    .await;
    // Pressed once its time is past: expired, nothing decided.
    let press = telegram.press(11, MARC, number, &format!("v:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Rien n’a été décidé.");
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "expired");
    // Said so, with its conversation, and no answer promised.
    let rewritten = message_numbered(&telegram, number).text;
    assert!(
        rewritten.starts_with(
            "L’action de cette demande montrée ci-dessous n’attend plus de décision (<a href="
        ),
        "{rewritten}"
    );
    assert!(!rewritten.contains("La suite"), "{rewritten}");
    assert!(
        rewritten.ends_with("</pre>\n<i>N’attend plus de décision.</i>"),
        "{rewritten}"
    );
    assert!(!rewritten.contains("à valider"), "{rewritten}");
    assert_eq!(
        keyboard_of(&telegram, number),
        json!({ "inline_keyboard": [] })
    );
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
