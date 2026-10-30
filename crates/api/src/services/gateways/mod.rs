//! The conversation gateways of a company (ADR-125): its Slack application,
//! declared by its connector administrators (`services::admin::gateways`).
//! Slack sends it the events of its workspace, each signed with its secret
//! (`events`); a person writing to it is offered a link to their account,
//! and links their Slack identity from their own session, Slack vouching
//! for their address (`link`); they list and remove it (`identities`). A
//! linked person then talks with the agent in Slack (ADR-127): their
//! message is answered in a conversation of theirs (`conversations`), and
//! the run's end or pause written in its thread (`replies`); an action
//! awaiting their own validation is validated or refused there with its
//! buttons (`choices`, ADR-129). A manager, or whom they named to stand in
//! for them, linked there is told in Slack of the requests awaiting them
//! and decides them with the same buttons (`approver_notices`, ADR-130).
//! Whatever comes from a person, Slack is first asked whether it comes from
//! their own direct conversation with the application (`direct`, ADR-131).
//! A company's Telegram bot receives its updates, proven by its webhook's
//! secret, and its people link their account with a code their session
//! asks for and confirms (`telegram`, ADR-133); a linked person then talks
//! with the agent there, answered in their private chat with the bot
//! (ADR-135), the run's end or pause written there by the same pass
//! (`replies`), and validates there what awaits them with its buttons
//! (ADR-136); a manager, or their stand-in, linked there is told there of
//! the requests awaiting them, by the same pass as in Slack
//! (`approver_notices`, ADR-137).
//! A company's Microsoft Teams bot receives the activities the Bot Framework
//! sends it, proven by the token Microsoft signs them with; a person
//! writing to it is sent a link to link their Teams identity, confirmed in
//! their own session once Teams says who they are (`teams`, `link`,
//! ADR-138).
//! The bot's token, the signing secret and the Teams bot's client secret
//! are sealed in the secret store.

mod approver_notices;
mod approver_text;
mod choices;
mod concerned;
mod conversations;
mod direct;
mod events;
mod identities;
mod link;
mod person;
mod private_chats;
mod replies;
mod reply_text;
mod retry;
mod started;
mod teams;
mod telegram;

use std::sync::Arc;

use mimorya_connectors::outbound::{NetworkPolicy, http_client};
use mimorya_connectors::slack::SlackApi;
use mimorya_connectors::teams::{TeamsApi, TeamsEndpoints};
use mimorya_connectors::telegram::TelegramApi;
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use crate::secrets::{SecretCipher, SecretRead, SecretStore};
use crate::services::background::BackgroundTasks;

pub use approver_notices::{APPROVAL_NOTIFIED, NOTIFIED_WITHIN, notify_approvers};
pub use choices::{PersonChoice, decide_in_slack};
pub use conversations::converse;
pub use events::{EventOutcome, PersonMessage, SlackHeaders};
pub use identities::GatewayOfPerson;
pub use link::{LINK_LIFETIME, LinkPreview};
pub use replies::reply_to_finished_runs;
pub use retry::MAX_ATTEMPTS;
pub use teams::{ActivityOutcome, TeamsMessage, converse_in_teams};
pub use telegram::{
    ChatMessage, PendingLink, TelegramChoice, TelegramLinkCode, UpdateOutcome,
    converse_in_telegram, decide_in_telegram,
};

/// Where Slack sends a gateway's events, under the API's address: with the
/// SAML reception, the inbound hooks' and a Telegram bot's updates, the
/// only routes a request without the browser's header reaches
/// (`plugins/csrf.rs`).
pub const SLACK_EVENTS_PREFIX: &str = "/gateways/slack/";
/// Where Telegram sends a bot's updates, under the API's address: the
/// other route a request without the browser's header reaches (ADR-133).
pub const TELEGRAM_UPDATES_PREFIX: &str = "/gateways/telegram/";
/// Where the Bot Framework sends a Teams bot's activities (its messaging
/// endpoint), under the API's address: the last route a request without
/// the browser's header reaches (ADR-138).
pub const TEAMS_ACTIVITIES_PREFIX: &str = "/gateways/teams/";
/// The page where a person confirms the link Slack gave them.
pub const LINK_PAGE: &str = "/account/gateway-link";

/// The secret store's name of a gateway's bot token.
pub fn bot_token_name(gateway_id: Uuid) -> String {
    format!("gateway.{gateway_id}.bot_token")
}

/// The secret store's name of a gateway's signing secret.
pub fn signing_secret_name(gateway_id: Uuid) -> String {
    format!("gateway.{gateway_id}.signing_secret")
}

/// The secret store's name of a Telegram bot's webhook secret (ADR-133).
pub fn webhook_secret_name(gateway_id: Uuid) -> String {
    format!("gateway.{gateway_id}.webhook_secret")
}

/// The secret store's name of a Teams bot's client secret (ADR-138).
pub fn client_secret_name(gateway_id: Uuid) -> String {
    format!("gateway.{gateway_id}.client_secret")
}

pub struct GatewayService {
    db: Database,
    secrets: SecretStore,
    cipher: Option<Arc<SecretCipher>>,
    slack: SlackApi,
    telegram: TelegramApi,
    teams: TeamsApi,
    web_origin: String,
    assistant_name: String,
    background: Arc<BackgroundTasks>,
}

/// What the gateways are built with.
pub struct GatewaySettings {
    /// The network Slack is reached on.
    pub network: NetworkPolicy,
    /// Slack's Web API (`SLACK_API`, a local one in tests).
    pub slack_api: String,
    /// Telegram's Bot API (`TELEGRAM_API`, a local one in tests).
    pub telegram_api: String,
    /// Microsoft, as a Teams bot reaches it (a local one in tests).
    pub teams: TeamsEndpoints,
    pub web_origin: String,
    pub assistant_name: String,
}

impl GatewayService {
    pub fn new(
        db: Database,
        cipher: Option<Arc<SecretCipher>>,
        settings: GatewaySettings,
        background: Arc<BackgroundTasks>,
    ) -> anyhow::Result<Self> {
        let slack = SlackApi::new(&settings.slack_api, http_client(settings.network)?)?;
        let telegram = TelegramApi::new(&settings.telegram_api, http_client(settings.network)?)?;
        let teams = TeamsApi::new(settings.teams, http_client(settings.network)?)?;
        Ok(Self {
            db,
            secrets: SecretStore::new(cipher.clone()),
            cipher,
            slack,
            telegram,
            teams,
            web_origin: settings.web_origin,
            assistant_name: settings.assistant_name,
            background,
        })
    }

    /// Slack's Web API.
    pub fn slack(&self) -> &SlackApi {
        &self.slack
    }

    /// Telegram's Bot API.
    pub fn telegram(&self) -> &TelegramApi {
        &self.telegram
    }

    /// Microsoft, as a Teams bot reaches it.
    pub fn teams(&self) -> &TeamsApi {
        &self.teams
    }

    pub fn secrets(&self) -> &SecretStore {
        &self.secrets
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    /// The address of a conversation in the application.
    fn conversation_url(&self, conversation_id: Uuid) -> String {
        format!("{}/chat/{conversation_id}", self.web_origin)
    }

    /// The address of the « Validations » screen.
    fn approvals_url(&self) -> String {
        format!("{}/approvals", self.web_origin)
    }

    /// The address to give Slack for the gateway's events.
    pub fn events_url(&self, gateway_id: Uuid) -> String {
        format!("{}/api{SLACK_EVENTS_PREFIX}{gateway_id}", self.web_origin)
    }

    /// The address Telegram sends the bot's updates to (ADR-133).
    pub fn telegram_updates_url(&self, gateway_id: Uuid) -> String {
        format!(
            "{}/api{TELEGRAM_UPDATES_PREFIX}{gateway_id}",
            self.web_origin
        )
    }

    /// The address the Bot Framework sends a Teams bot's activities to, its
    /// messaging endpoint (ADR-138).
    pub fn teams_activities_url(&self, gateway_id: Uuid) -> String {
        format!(
            "{}/api{TEAMS_ACTIVITIES_PREFIX}{gateway_id}",
            self.web_origin
        )
    }

    /// The gateway's secret `name`, if it is stored and opens.
    async fn secret(
        &self,
        tx: &mut TenantTx,
        tenant_id: Uuid,
        name: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(match self.secrets.read(tx, tenant_id, name).await? {
            SecretRead::Value(value) => Some(value),
            _ => None,
        })
    }
}
