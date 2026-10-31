//! The company's Microsoft Teams bot as its gateway (ADR-138): the
//! activities the Bot Framework sends it, proven by the token Microsoft
//! signs them with (`activities`); a person writing to it is sent a link to
//! link their Teams identity to their account, confirmed in their own
//! session once Teams says who they are (`link`). A linked person's message
//! is then read as a command or words for the agent (`answers`, ADR-140),
//! their words answered by the agent in a conversation of theirs held in
//! their personal conversation with the bot (`conversations`), the run's
//! end or pause written there (`reply_text`, by `replies`), the actions
//! awaiting their own validation on a card whose buttons decide them
//! (`choices`, ADR-141); a manager, or whom they named to stand in for
//! them, told there of the requests awaiting them, those that may be
//! validated there on a card too (`approver_text`, by `approver_notices`,
//! ADR-142). What the bot says of itself (`texts`) is written in the
//! conversation the activity came from, in the background.

mod activities;
mod answers;
mod approver_text;
mod choices;
mod conversations;
mod reply_text;
mod texts;
mod written;

use mimorya_core::gateways::teams::TeamsApplication;
use uuid::Uuid;

pub use activities::ActivityOutcome;
pub(super) use approver_text::approver_notice as teams_approver_notice;
pub use choices::{TeamsChoice, decide_in_teams};
pub use conversations::{TeamsMessage, converse_in_teams};
pub(super) use reply_text::{Pause as TeamsPause, reply_text as teams_reply_text};
pub(super) use written::{after_failure as teams_after_failure, keep_card as keep_teams_card};

use super::GatewayService;

impl GatewayService {
    /// Writes `text` (Teams' Markdown) in the conversation `conversation`
    /// of its service, as the bot `app`; in the background, once the
    /// activity is answered.
    pub(super) fn say_in_teams(
        &self,
        gateway_id: Uuid,
        app: TeamsApplication,
        (service_url, conversation): (String, String),
        text: String,
    ) {
        let teams = self.teams.clone();
        self.background.spawn(async move {
            let sent = teams.send(&app, (&service_url, &conversation), &text).await;
            if let Err(failure) = sent {
                let failure = failure.message();
                tracing::warn!(gateway = %gateway_id, %failure, "Teams message not sent");
            }
        });
    }
}
