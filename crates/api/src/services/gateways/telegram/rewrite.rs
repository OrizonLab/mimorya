//! A message of the bot whose button was pressed, written again once some
//! of its actions are settled (ADR-148): its text, each action as it stands
//! (`settled_text`), with the words of a pause or a notice, whom the agent
//! acts for and the absent manager named as they are now; its buttons,
//! those of the actions settled taken off, the others as they were. Then
//! their statuses are read once more: written meanwhile by another press
//! from an older state, it is written again as it now is, as a Teams card
//! (ADR-141). Only a message the platform kept (`kept`), whose actions its
//! presser all sees, is written again: any other (written before it was
//! kept, or showing an action they do not see, a Telegram account linked
//! since to someone else learning nothing of the actions of the person
//! before) only has the buttons taken off of the actions decided by then
//! that they see, as before (ADR-136).

use mimorya_core::gateways::telegram::{buttons_in, without_decided};
use mimorya_core::gateways::{ActionState, ShownAction, to_write_again};
use mimorya_db::repositories::approvals::{ApprovalRow, get_approval};
use mimorya_db::repositories::gateway_cards::{GatewayCard, find_gateway_card};
use serde_json::Value;
use uuid::Uuid;

use super::choices::TelegramChoice;
use super::reply_text::{Markup, Pause};
use super::settled_text::{notice_again, pause_again};
use crate::app::Services;
use crate::services::gateways::approver_text::{ApproverNotice, notice_names};
use crate::services::gateways::concerned::{Concerned, shown_action};

/// Writes again the message `pressed` was pressed under, once some of its
/// actions are settled; only its buttons when it was not kept, or shows an
/// action its presser does not see.
pub(super) async fn write_again(
    services: &Services,
    pressed: &TelegramChoice,
    concerned: &Concerned,
) -> anyhow::Result<()> {
    let press = &pressed.press;
    let (chat, number) = (press.person.id.to_string(), press.message_id.to_string());
    let mut tx = services.db.begin(pressed.tenant_id).await?;
    let kept = find_gateway_card(&mut tx, pressed.gateway_id, &chat, &number).await?;
    tx.commit().await?;
    let Some(kept) = kept else {
        return take_off_decided(services, pressed, concerned).await;
    };
    let ids = kept.approval_ids.as_slice();
    let mut written: Option<Vec<ActionState>> = None;
    for _ in 0..=ids.len() {
        let Some(approvals) = concerned
            .shown_now(services, pressed.tenant_id, ids)
            .await?
        else {
            return take_off_decided(services, pressed, concerned).await;
        };
        let states: Vec<ActionState> = (approvals.iter())
            .map(|a| ActionState::of(&a.status))
            .collect();
        if !to_write_again(&states, written.as_deref()) {
            return Ok(());
        }
        write_message(services, pressed, &kept, (&approvals, &states)).await?;
        written = Some(states);
    }
    Ok(())
}

/// Writes the message `kept` again in place, its `approvals` standing as
/// `states`: in HTML, or in plain text once Telegram refused it.
async fn write_message(
    services: &Services,
    pressed: &TelegramChoice,
    kept: &GatewayCard,
    (approvals, states): (&[ApprovalRow], &[ActionState]),
) -> anyhow::Result<()> {
    let shown: Vec<ShownAction> = approvals.iter().map(shown_action).collect();
    let in_application = usize::try_from(kept.in_application).unwrap_or(0);
    let (html, plain) = match kept.conversation_id {
        Some(conversation) => {
            let url = services.gateways.conversation_url(conversation);
            let pause = Pause {
                shown: &shown,
                in_application,
                managed: usize::try_from(kept.managed).unwrap_or(0),
            };
            let text = |markup| pause_again(markup, &url, pause, states);
            (text(Markup::Html), text(Markup::Plain))
        }
        // A notice's, to a manager (ADR-137); one request shown at least,
        // all of one run.
        None => {
            let Some(first) = approvals.first() else {
                anyhow::bail!("a notice's message shows no request");
            };
            let tenant_id = pressed.tenant_id;
            let (requested_for, absent) =
                notice_names(services, tenant_id, first, kept.notice_for).await?;
            let link = services.gateways.approvals_url();
            let notice = ApproverNotice {
                requested_for: &requested_for,
                absent: absent.as_deref(),
                shown: &shown,
                in_application,
                link: &link,
            };
            let text = |markup| notice_again(markup, &notice, states);
            (text(Markup::Html), text(Markup::Plain))
        }
    };
    let settled: Vec<Uuid> = (approvals.iter().zip(states))
        .filter(|(_, state)| **state != ActionState::Open)
        .map(|(approval, _)| approval.id)
        .collect();
    let keyboard = without_decided(&pressed.press.keyboard, &settled);
    edit(services, pressed, (&html, &plain), &keyboard).await
}

/// Writes `pressed`'s message again as `html`, or as `plain` once Telegram
/// refused its markup, with `keyboard`'s buttons; as it already was, it is
/// as asked.
async fn edit(
    services: &Services,
    pressed: &TelegramChoice,
    (html, plain): (&str, &str),
    keyboard: &Value,
) -> anyhow::Result<()> {
    let telegram = services.gateways.telegram();
    let press = &pressed.press;
    let to = (press.person.id, press.message_id);
    let token = &pressed.token;
    let written = match telegram
        .edit_message(token, to, (html, true), keyboard)
        .await
    {
        Err(failure) if failure.markup_refused() => {
            tracing::warn!(gateway = %pressed.gateway_id, "Telegram refused the message's markup");
            telegram
                .edit_message(token, to, (plain, false), keyboard)
                .await
        }
        written => written,
    };
    match written {
        Err(failure) if !failure.unchanged() => {
            anyhow::bail!("Telegram refused the message: {}", failure.message())
        }
        _ => Ok(()),
    }
}

/// Takes off the message the buttons of its actions decided by now that
/// its presser sees: the one just decided, and any other decided meanwhile;
/// the others stay. A message none of whose actions is decided is left as
/// it is.
async fn take_off_decided(
    services: &Services,
    pressed: &TelegramChoice,
    concerned: &Concerned,
) -> anyhow::Result<()> {
    let press = &pressed.press;
    let mut decided = Vec::new();
    let mut tx = services.db.begin(pressed.tenant_id).await?;
    for id in buttons_in(&press.keyboard) {
        let approval = get_approval(&mut tx, id).await?;
        if approval.is_some_and(|a| concerned.sees(&a) && a.status != "pending") {
            decided.push(id);
        }
    }
    tx.commit().await?;
    if decided.is_empty() {
        return Ok(());
    }
    let keyboard = without_decided(&press.keyboard, &decided);
    let telegram = services.gateways.telegram();
    let (chat, message) = (press.person.id, press.message_id);
    if let Err(failure) = telegram
        .edit_buttons(&pressed.token, chat, message, &keyboard)
        .await
    {
        anyhow::bail!("Telegram buttons not edited: {}", failure.message());
    }
    Ok(())
}
