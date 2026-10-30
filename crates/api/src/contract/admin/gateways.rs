//! A company's conversation gateways as the administration declares and
//! reads them (`connector:manage`, ADR-125): its Slack application, what
//! Slack said of it, the address to give Slack for its events, and how many
//! people linked their Slack identity. Its token and secret are never
//! returned, only whether they are stored.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::{trimmed, trimmed_opt};

/// `GET /admin/gateways`.
#[derive(Debug, Serialize, TS)]
pub struct Gateways {
    pub gateways: Vec<GatewayView>,
    /// The bot token scopes a Slack application is given.
    #[serde(rename = "slackScopes")]
    pub slack_scopes: Vec<String>,
    /// The bot events it subscribes to.
    #[serde(rename = "slackEvents")]
    pub slack_events: Vec<String>,
}

/// A gateway of the company.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayView {
    pub id: Uuid,
    /// `slack`, `telegram` or `teams`.
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    /// The workspace Slack said the application is installed in; the
    /// Telegram bot's identifier; the Teams bot's Microsoft Entra directory.
    pub workspace_id: String,
    /// Its name; the Telegram bot's username; the Teams bot's directory.
    pub workspace_name: String,
    /// The application's bot user; the Telegram bot's identifier; the Teams
    /// bot's application (Microsoft App ID).
    pub bot_user_id: String,
    /// Where Slack sends the application's events (« Event Subscriptions »);
    /// where Telegram sends the bot's updates, which the platform told it;
    /// the Teams bot's messaging endpoint, to give the Azure bot.
    pub events_url: String,
    /// Whether its bot's token and its signing secret (Slack) or its
    /// webhook's secret (Telegram), or its client secret (Teams), are
    /// stored.
    pub secrets_stored: bool,
    /// How many people linked their identity.
    pub identities: i64,
    /// The permissions the application asks for that Slack said its token
    /// lacks (ADR-132); `None` when Slack has not said its permissions.
    pub missing_scopes: Option<Vec<String>>,
    /// Those Slack said it has beyond what it asks for: a token stronger
    /// than the platform needs.
    pub extra_scopes: Option<Vec<String>>,
    /// When Slack was last asked what the token is.
    pub scopes_checked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Body of `POST /admin/gateways/slack`: the company's Slack application,
/// installed in its workspace.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewSlackGateway {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    /// Its « Bot User OAuth Token » (`xoxb-…`).
    #[serde(deserialize_with = "trimmed")]
    pub bot_token: String,
    /// Its « Signing Secret ».
    #[serde(deserialize_with = "trimmed")]
    pub signing_secret: String,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

/// Body of `POST /admin/gateways/telegram`: the company's Telegram bot
/// (ADR-133).
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewTelegramGateway {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    /// The token BotFather gave the bot (`123456789:…`).
    #[serde(deserialize_with = "trimmed")]
    pub bot_token: String,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

/// Body of `POST /admin/gateways/teams`: the company's Microsoft Teams bot
/// (ADR-138).
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewTeamsGateway {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    /// The Azure bot's application (Microsoft App ID).
    #[serde(deserialize_with = "trimmed")]
    pub app_id: String,
    /// The Microsoft Entra directory (tenant) of the bot and the company.
    #[serde(deserialize_with = "trimmed")]
    pub directory_id: String,
    /// The application's client secret.
    #[serde(deserialize_with = "trimmed")]
    pub client_secret: String,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}

/// Body of `PATCH /admin/gateways/{id}`: an absent field is left as it
/// stands.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayUpdate {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 2, max = 80))]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
    /// A new token of the same application, or of one of the same
    /// workspace; of the same Telegram bot.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub bot_token: Option<String>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub signing_secret: Option<String>,
    /// A new client secret of the Teams bot's application.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub client_secret: Option<String>,
}
