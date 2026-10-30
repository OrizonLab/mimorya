//! The company's Microsoft Teams bot as its gateway (ADR-138): the
//! activities the Bot Framework sends it, proven by the token Microsoft
//! signs them with (`activities`); a person writing to it is sent a link to
//! link their Teams identity to their account, confirmed in their own
//! session once Teams says who they are (`link`); what the bot says of
//! itself (`texts`), written in the conversation the activity came from, in
//! the background.

mod activities;
mod texts;

use mimorya_core::gateways::teams::TeamsApplication;
use uuid::Uuid;

pub use activities::ActivityOutcome;

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
