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
//! The bot's token and the signing secret are sealed in the secret store.

mod approver_notices;
mod approver_text;
mod choices;
mod conversations;
mod direct;
mod events;
mod identities;
mod link;
mod person;
mod replies;
mod reply_text;

use std::sync::Arc;

use mimorya_connectors::outbound::{NetworkPolicy, http_client};
use mimorya_connectors::slack::SlackApi;
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use crate::secrets::{SecretCipher, SecretRead, SecretStore};
use crate::services::background::BackgroundTasks;

pub use approver_notices::{NOTIFIED_IN_SLACK, NOTIFIED_WITHIN, notify_approvers_in_slack};
pub use choices::{PersonChoice, decide_in_slack};
pub use conversations::converse;
pub use events::{EventOutcome, PersonMessage, SlackHeaders};
pub use identities::GatewayOfPerson;
pub use link::{LINK_LIFETIME, LinkPreview};
pub use replies::{MAX_ATTEMPTS, reply_to_finished_runs};

/// Where Slack sends a gateway's events, under the API's address: the only
/// route besides the SAML reception and the inbound hooks' that a request
/// without the browser's header reaches (`plugins/csrf.rs`).
pub const SLACK_EVENTS_PREFIX: &str = "/gateways/slack/";
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

pub struct GatewayService {
    db: Database,
    secrets: SecretStore,
    cipher: Option<Arc<SecretCipher>>,
    slack: SlackApi,
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
        Ok(Self {
            db,
            secrets: SecretStore::new(cipher.clone()),
            cipher,
            slack,
            web_origin: settings.web_origin,
            assistant_name: settings.assistant_name,
            background,
        })
    }

    /// Slack's Web API.
    pub fn slack(&self) -> &SlackApi {
        &self.slack
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
