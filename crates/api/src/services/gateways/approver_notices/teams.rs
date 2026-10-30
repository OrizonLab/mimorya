//! The requests awaiting a manager told in Teams (ADR-142): a message in
//! their personal conversation with the company's bot, in the Markdown
//! Teams reads (`teams_approver_notice`), the actions that may be validated
//! there shown on the card it comes with, as many as one card holds, with
//! their exact parameters and buttons (ADR-141); the card kept, so that its
//! buttons decide. The bot writes only in a conversation Teams knows: the
//! one the bot last met the person in, kept with their identity; one linked
//! before it was kept, who has not written to the bot since, is written
//! nothing. The Teams account the bot met is theirs: Teams named its Entra
//! object with the account's address when they linked it (ADR-138).

use mimorya_core::gateways::teams::{
    CardAction, CardState, TeamsApplication, shown_in_card, validation_card,
};
use mimorya_db::repositories::gateway_approval_notices::GatewayApprovalNotice;
use mimorya_db::repositories::gateway_cards::{CardPlace, WrittenCard};
use mimorya_db::repositories::gateway_replies::ReplyRecord;
use uuid::Uuid;

use super::{Reach, Told, shown_action};
use crate::app::Services;
use crate::services::gateways::replies::{after_teams_failure, keep_teams_card};
use crate::services::gateways::teams::teams_approver_notice;

/// Writes `told.to`, in their personal conversation with the bot, the
/// requests `theirs`; what became of the message.
pub(super) async fn tell(
    services: &Services,
    told: &Told<'_>,
    reach: Reach<'_>,
    theirs: &[GatewayApprovalNotice],
) -> ReplyRecord {
    let gateway = told.gateway;
    let Some((service_url, chat)) = reach.conversation else {
        // Met in no conversation since it was kept: nowhere to write.
        return ReplyRecord::Done("skipped");
    };
    let shown = shown_in_card(theirs.iter().map(shown_action));
    let link = services.gateways.approvals_url();
    let text = teams_approver_notice(&told.notice(theirs, &shown, &link));
    let app = TeamsApplication {
        app_id: gateway.bot_user_id.clone(),
        directory_id: gateway.workspace_id.clone(),
        client_secret: reach.token.to_owned(),
    };
    let to = (service_url, chat);
    let teams = services.gateways.teams();
    if shown.is_empty() {
        return match teams.send(&app, to, &text).await {
            Ok(()) => ReplyRecord::Done("sent"),
            Err(failure) => after_teams_failure(&failure, reach.attempt, reach.now),
        };
    }
    let actions: Vec<CardAction> = (shown.iter())
        .map(|&shown| CardAction {
            shown,
            state: CardState::Open,
        })
        .collect();
    match teams
        .send_card(&app, to, &text, &validation_card(&actions))
        .await
    {
        Ok(activity) => {
            let written = activity.as_deref().map(|activity_id| WrittenCard {
                tenant_id: told.tenant_id,
                gateway_id: gateway.id,
                chat,
                activity_id,
                place: CardPlace::Notice(told.to.user_id),
            });
            let ids: Vec<Uuid> = shown.iter().map(|a| a.approval_id).collect();
            let in_application = i32::try_from(theirs.len() - shown.len()).unwrap_or(i32::MAX);
            keep_teams_card(&services.gateways, written, &ids, (in_application, 0)).await;
            ReplyRecord::Done("sent")
        }
        Err(failure) => after_teams_failure(&failure, reach.attempt, reach.now),
    }
}
