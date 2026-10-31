//! An end or pause answered in the person's personal conversation with the
//! company's Teams bot (ADR-140), where its conversation is held, in the
//! Markdown Teams reads (`teams::reply_text`), at the Teams service its
//! latest message came from, as the bot's application. A pause shows the
//! actions awaiting the person's own validation that may be given there on
//! a card, with their exact parameters and their buttons (ADR-141), as many
//! as one card holds (`shown_in_card`); the card is kept, with the requests
//! it shows, once Teams took it. The others are validated in the
//! application, which shows them whole. What Teams refuses (the bot blocked
//! or removed, the conversation unknown, the application's secret refused)
//! is not tried again, nor a service that is not Microsoft's; Microsoft
//! unreachable, failing or answered unread (Entra's token), tried again a
//! few times (`teams::written`).

use chrono::{DateTime, Utc};
use mimorya_core::gateways::ActionState;
use mimorya_core::gateways::teams::{CardAction, TeamsApplication, shown_in_card, validation_card};
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_cards::{CardPlace, WrittenCard};
use mimorya_db::repositories::gateway_replies::{ReplyRecord, ReplyTarget};
use uuid::Uuid;

use crate::services::gateways::GatewayService;
use crate::services::gateways::teams::{
    TeamsPause, keep_teams_card, shown_action, teams_after_failure, teams_reply_text,
};

/// Writes the answer to `target`, of the company `tenant_id`, in the
/// person's personal conversation with the bot, whose application's secret
/// is `client_secret`; `awaiting`, a pause's actions still awaiting a
/// decision. What became of it.
pub(super) async fn answer(
    gateways: &GatewayService,
    (tenant_id, target): (Uuid, &ReplyTarget),
    awaiting: &[ApprovalRow],
    client_secret: String,
    now: DateTime<Utc>,
) -> ReplyRecord {
    // A Teams thread always has one (the base requires it).
    let Some(service_url) = target.service_url.as_deref() else {
        tracing::warn!(gateway = %target.gateway_id, "Teams conversation's service unknown");
        return ReplyRecord::Done("failed");
    };
    let own: Vec<&ApprovalRow> = (awaiting.iter())
        .filter(|a| !a.routed_to_manager())
        .collect();
    let paused = target.status == "awaiting_approval";
    let shown = if paused {
        shown_in_card(own.iter().map(|a| shown_action(a)))
    } else {
        Vec::new()
    };
    let pause = TeamsPause {
        shown: shown.len(),
        in_application: own.len() - shown.len(),
        managed: awaiting.len() - own.len(),
    };
    let url = gateways.conversation_url(target.conversation_id);
    let text = teams_reply_text(&target.status, target.answer.as_deref(), &url, pause);
    let app = TeamsApplication {
        app_id: target.bot_user_id.clone(),
        directory_id: target.workspace_id.clone(),
        client_secret,
    };
    let to = (service_url, target.channel_id.as_str());
    let teams = gateways.teams();
    if shown.is_empty() {
        return match teams.send(&app, to, &text).await {
            Ok(()) => ReplyRecord::Done("sent"),
            Err(failure) => teams_after_failure(&failure, target.attempts + 1, now),
        };
    }
    let actions: Vec<CardAction> = (shown.iter())
        .map(|&shown| CardAction {
            shown,
            state: ActionState::Open,
        })
        .collect();
    let ids: Vec<Uuid> = shown.iter().map(|a| a.approval_id).collect();
    let others = (
        i32::try_from(pause.in_application).unwrap_or(i32::MAX),
        i32::try_from(pause.managed).unwrap_or(i32::MAX),
    );
    match teams
        .send_card(&app, to, &text, &validation_card(&actions))
        .await
    {
        Ok(activity) => {
            let written = activity.as_deref().map(|activity_id| WrittenCard {
                tenant_id,
                gateway_id: target.gateway_id,
                chat: &target.channel_id,
                activity_id,
                place: CardPlace::Thread(target.thread_id),
            });
            keep_teams_card(gateways, target.gateway_id, written, &ids, others).await;
            ReplyRecord::Done("sent")
        }
        Err(failure) => teams_after_failure(&failure, target.attempts + 1, now),
    }
}
