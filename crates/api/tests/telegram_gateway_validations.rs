//! Validations given in Telegram (ADR-136): a run started there that
//! pauses for its author's own validation shows the action with its exact
//! parameters and its buttons; pressed, it is decided as its author,
//! audited so, the press answered, the message written again with the
//! action said as settled and without its buttons (ADR-148), the outcome
//! said in answer to the message, and the run goes on. Only a button the
//! bot showed under its own message decides, and only for whom the request
//! awaits: a button no message carries, a message not the bot's, a press in
//! another's chat, another linked person's press of it, a Telegram account
//! no longer linked, a paused bot decide nothing. An action decided
//! meanwhile in the application is written again so, but never by someone
//! it is not for. An action sent outside the company, or one a
//! manager decides, is shown without buttons, and no press decides it.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeTelegram, TELEGRAM_BOT};
use mimorya_core::demo::user;
use serde_json::json;
use support::TestApi;
use support::manager_approvals::{listed, route_to_managers};
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use support::slack_conversations::answer;
use support::sso::SSO_DEPLOYMENT;
use support::telegram::{
    LEA_TELEGRAM, LEA_USERNAME, MARC_TELEGRAM, MARC_USERNAME, ask_code, confirm, open_and_accept,
};
use support::telegram_conversations::{
    claimed_press, deliver, keyboard_of, last_brief, last_said, latest_number, linked_bot,
    message_numbered,
};
use support::telegram_validations::asked;
use uuid::Uuid;

const MARC: (i64, &str) = (MARC_TELEGRAM, MARC_USERNAME);
const LEA: (i64, &str) = (LEA_TELEGRAM, LEA_USERNAME);
/// Nadia's Telegram account.
const NADIA_TELEGRAM: i64 = 6161;
const NADIA: (i64, &str) = (NADIA_TELEGRAM, "nadia_ben");

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_awaiting_its_author_is_validated_with_its_button() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let end = "Planning de INT-2052 appliqué.";
    let (run, approval) = asked(&api, &telegram, (id, MARC), end).await;

    // The pause: the action, its exact parameters, its buttons.
    assert_eq!(answer(&api).await, 1);
    let pause = last_said(&telegram, MARC_TELEGRAM);
    let number = latest_number(&telegram);
    assert!(pause.html);
    assert!(
        pause.text.contains("attend votre validation"),
        "{}",
        pause.text
    );
    assert!(
        pause.text.contains("<b>Action à valider :</b>"),
        "{}",
        pause.text
    );
    assert!(
        pause.text.contains("Paramètres exacts qui seront exécutés"),
        "{}",
        pause.text
    );
    assert!(pause.text.contains("<pre>"), "{}", pause.text);
    let keyboard = keyboard_of(&telegram, number);
    assert_eq!(keyboard["inline_keyboard"][0][0]["text"], "Valider");
    assert_eq!(
        keyboard["inline_keyboard"][0][0]["callback_data"],
        format!("v:{approval}")
    );
    assert_eq!(
        keyboard["inline_keyboard"][0][1]["callback_data"],
        format!("r:{approval}")
    );

    // Validated in Telegram: decided as him, audited so, the press
    // answered, its buttons taken off, the outcome said in answer.
    let since = latest_entry(&api).await;
    let press = telegram.press(11, MARC, number, &format!("v:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Validée");
    let said = last_said(&telegram, MARC_TELEGRAM);
    assert_eq!(said.reply_to, Some(number));
    assert_eq!(
        said.text,
        "Validée par vous dans Telegram. La demande reprend : la suite viendra ici."
    );
    assert_eq!(
        keyboard_of(&telegram, number),
        json!({ "inline_keyboard": [] })
    );
    // Its message written again: the action settled, no longer to
    // validate, its answer still to come here.
    let rewritten = message_numbered(&telegram, number);
    assert_eq!(rewritten.edits, 1);
    assert!(rewritten.html);
    assert!(
        rewritten
            .text
            .starts_with("L’action de cette demande montrée ci-dessous est décidée (<a href=\""),
        "{}",
        rewritten.text
    );
    assert!(
        rewritten
            .text
            .contains("). La suite viendra ici.\n\n<b>Action :</b> "),
        "{}",
        rewritten.text
    );
    assert!(
        rewritten.text.contains("Paramètres exacts :\n<pre>"),
        "{}",
        rewritten.text
    );
    assert!(
        rewritten.text.ends_with("</pre>\n<i>Validée.</i>"),
        "{}",
        rewritten.text
    );
    assert!(!rewritten.text.contains("à valider"), "{}", rewritten.text);
    assert!(
        !rewritten.text.contains("qui seront exécutés"),
        "{}",
        rewritten.text
    );
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "approved");
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("approval.approved"), "{audited}");
    assert!(audited.contains(r#""gateway":"telegram""#), "{audited}");

    // The request went on: its end written in his chat.
    assert_eq!(answer(&api).await, 1);
    assert!(last_said(&telegram, MARC_TELEGRAM).text.starts_with(end));

    // Pressed again: nothing more decided.
    let again = telegram.press(12, MARC, number, &format!("r:{approval}"));
    deliver(&api, &telegram, id, &again).await;
    assert_eq!(last_brief(&telegram), "Ce bouton n’est plus valable.");
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "approved");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_refused_with_its_button_is_not_carried_out() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (run, approval) = asked(&api, &telegram, (id, MARC), "Rien n’a été appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    let press = telegram.press(11, MARC, number, &format!("r:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Refusée");
    let said = last_said(&telegram, MARC_TELEGRAM).text;
    assert!(
        said.starts_with("Refusée par vous dans Telegram."),
        "{said}"
    );
    let rewritten = message_numbered(&telegram, number).text;
    assert!(rewritten.ends_with("<i>Refusée.</i>"), "{rewritten}");
    assert!(!rewritten.contains("à valider"), "{rewritten}");
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "rejected");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_decided_meanwhile_has_its_buttons_taken_off() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (run, approval) = asked(&api, &telegram, (id, MARC), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    // Validated in the application first.
    let decided = marc
        .post(
            &format!("/approvals/{approval}/decision"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    // Its button pressed in Telegram then: nothing more decided, and its
    // message written again as the application decided it.
    let press = telegram.press(11, MARC, number, &format!("r:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Rien n’a été décidé.");
    let said = last_said(&telegram, MARC_TELEGRAM).text;
    assert_eq!(said, "Cette demande a déjà été traitée");
    assert_eq!(
        keyboard_of(&telegram, number),
        json!({ "inline_keyboard": [] })
    );
    let rewritten = message_numbered(&telegram, number).text;
    assert!(rewritten.ends_with("<i>Validée.</i>"), "{rewritten}");
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "approved");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_decided_action_s_buttons_are_left_to_whom_it_is_for() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (_, approval) = asked(&api, &telegram, (id, MARC), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    let path = format!("/approvals/{approval}/decision");
    let decided = marc.post(&path, json!({ "approved": true })).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    // His Telegram account linked since to Léa: its former pause's buttons
    // pressed tell her nothing of his request, nor take them off.
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    let lea = api.sign_in(user::LEA).await;
    let code = ask_code(&lea, id).await;
    open_and_accept(&api, &telegram, id, 20, MARC, &code).await;
    assert_eq!(confirm(&lea, id).await.status, StatusCode::OK);
    let press = telegram.press(22, MARC, number, &format!("v:{approval}"));
    deliver(&api, &telegram, id, &press).await;
    assert_eq!(last_brief(&telegram), "Rien n’a été décidé.");
    let said = last_said(&telegram, MARC_TELEGRAM).text;
    assert_eq!(said, "Demande de validation introuvable");
    let buttons = &keyboard_of(&telegram, number)["inline_keyboard"][0];
    assert_eq!(buttons.as_array().map(Vec::len), Some(2));
    // Nor is his message written again for her.
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
async fn only_a_button_the_bot_showed_decides_and_only_for_whom_it_awaits() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (run, approval) = asked(&api, &telegram, (id, MARC), "Appliqué.").await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    let pending = || async { listed(&marc, &run).await.unwrap()["status"].clone() };

    // A button its message does not carry, as a client may claim.
    let other = Uuid::new_v4();
    let unshown = telegram.press(11, MARC, number, &format!("v:{other}"));
    deliver(&api, &telegram, id, &unshown).await;
    assert_eq!(last_brief(&telegram), "Ce bouton n’est plus valable.");
    // A message the bot did not send.
    let not_the_bot = claimed_press(12, MARC, (number, MARC_TELEGRAM), approval, true);
    deliver(&api, &telegram, id, &not_the_bot).await;
    assert_eq!(last_brief(&telegram), "Ce bouton n’est plus valable.");
    // Pressed in his chat by someone else: not read at all.
    let answers = telegram.answers().len();
    let theirs = telegram.press(13, LEA, number, &format!("v:{approval}"));
    deliver(&api, &telegram, id, &theirs).await;
    assert_eq!(telegram.answers().len(), answers);
    assert_eq!(pending().await, "pending");

    // A request that does not exist, shown as if the bot had: nothing told.
    let unknown = claimed_press(14, MARC, (number, TELEGRAM_BOT), other, true);
    deliver(&api, &telegram, id, &unknown).await;
    assert_eq!(last_brief(&telegram), "Rien n’a été décidé.");
    assert_eq!(
        last_said(&telegram, MARC_TELEGRAM).text,
        "Demande de validation introuvable"
    );
    // Marc's, pressed by Léa, linked too, as if the bot had shown it to her:
    // told of it no more than of one that does not exist.
    let lea = api.sign_in(user::LEA).await;
    let code = ask_code(&lea, id).await;
    open_and_accept(&api, &telegram, id, 20, LEA, &code).await;
    assert_eq!(confirm(&lea, id).await.status, StatusCode::OK);
    let hers = claimed_press(22, LEA, (number, TELEGRAM_BOT), approval, true);
    deliver(&api, &telegram, id, &hers).await;
    assert_eq!(last_brief(&telegram), "Rien n’a été décidé.");
    assert_eq!(
        last_said(&telegram, LEA_TELEGRAM).text,
        "Demande de validation introuvable"
    );
    assert_eq!(pending().await, "pending");

    // The bot paused: nothing taken.
    let path = format!("/admin/gateways/{id}");
    assert_eq!(
        paul.patch(&path, json!({ "enabled": false })).await.status,
        StatusCode::OK
    );
    let paused = telegram.press(15, MARC, number, &format!("v:{approval}"));
    deliver(&api, &telegram, id, &paused).await;
    assert_eq!(telegram.answers().len(), answers + 2);
    assert_eq!(
        paul.patch(&path, json!({ "enabled": true })).await.status,
        StatusCode::OK
    );

    // His Telegram account no longer linked: told so, nothing decided.
    let removed = marc.delete(&format!("/account/gateways/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    let unlinked = telegram.press(16, MARC, number, &format!("v:{approval}"));
    deliver(&api, &telegram, id, &unlinked).await;
    assert_eq!(last_brief(&telegram), "Rien n’a été décidé.");
    let said = last_said(&telegram, MARC_TELEGRAM).text;
    assert!(
        said.contains("reliez d’abord votre compte Telegram"),
        "{said}"
    );
    assert_eq!(pending().await, "pending");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_sent_outside_the_company_has_no_button_and_no_press_decides_it() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = linked_bot(&api, &telegram, (&paul, &marc), MARC).await;
    let (run, approval) = asked(&api, &telegram, (id, MARC), "Appliqué.").await;
    // As an e-mail sent in his name would be.
    as_owner(
        "update approvals set risk = 'external' where id = $1",
        approval,
    )
    .await;
    assert_eq!(answer(&api).await, 1);
    let number = latest_number(&telegram);
    let pause = last_said(&telegram, MARC_TELEGRAM);
    assert_eq!(pause.keyboard, None);
    assert!(
        pause.text.contains("à donner dans l’application"),
        "{}",
        pause.text
    );
    // A press claiming its buttons decides nothing.
    let claimed = claimed_press(11, MARC, (number, TELEGRAM_BOT), approval, true);
    deliver(&api, &telegram, id, &claimed).await;
    let said = last_said(&telegram, MARC_TELEGRAM).text;
    assert!(
        said.starts_with("Cette action se valide dans l’application"),
        "{said}"
    );
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_a_manager_decides_has_no_button_and_no_press_of_hers_decides_it() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&SSO_DEPLOYMENT, &telegram.url).await;
    let (paul, nadia) = (
        api.sign_in(user::PAUL).await,
        api.sign_in(user::NADIA).await,
    );
    let id = linked_bot(&api, &telegram, (&paul, &nadia), NADIA).await;
    // Her manager, Marc, validates what she writes.
    route_to_managers(&paul, &["write"]).await;
    let (run, approval) = asked(&api, &telegram, (id, NADIA), "Appliqué.").await;
    route_to_managers(&paul, &[]).await;
    assert_eq!(answer(&api).await, 1);
    let pause = last_said(&telegram, NADIA_TELEGRAM);
    assert_eq!(pause.keyboard, None);
    assert!(
        pause.text.contains("validation de votre responsable"),
        "{}",
        pause.text
    );
    // Her pressing a button claimed for it, either one, decides nothing.
    let number = latest_number(&telegram);
    for (update_id, approved) in [(11, true), (12, false)] {
        let claimed = claimed_press(update_id, NADIA, (number, TELEGRAM_BOT), approval, approved);
        deliver(&api, &telegram, id, &claimed).await;
        let said = last_said(&telegram, NADIA_TELEGRAM).text;
        assert_eq!(
            said,
            "Cette action attend la validation de votre responsable : rien n’a été décidé."
        );
        assert_eq!(listed(&nadia, &run).await.unwrap()["status"], "pending");
    }
}
