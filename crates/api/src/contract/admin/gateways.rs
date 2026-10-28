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
    /// `slack`.
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    /// The workspace Slack said the application is installed in.
    pub workspace_id: String,
    pub workspace_name: String,
    /// The application's bot user.
    pub bot_user_id: String,
    /// Where Slack sends the application's events (« Event Subscriptions »).
    pub events_url: String,
    /// Whether its bot's token and its signing secret are stored.
    pub secrets_stored: bool,
    /// How many people linked their identity.
    pub identities: i64,
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
    /// workspace.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub bot_token: Option<String>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub signing_secret: Option<String>,
}
