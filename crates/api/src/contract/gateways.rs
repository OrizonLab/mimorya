//! The conversation gateways of the person's company, and who they are on
//! each (ADR-125): what a link Slack gave them would link, and its
//! confirmation; the link a Telegram bot is started with, and whom
//! Telegram sent its code back from (ADR-133).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::trimmed;

/// `GET /account/gateways`.
#[derive(Debug, Serialize, TS)]
pub struct AccountGateways {
    pub gateways: Vec<AccountGatewayView>,
}

/// A gateway of the company, and the person's identity there.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountGatewayView {
    pub gateway_id: Uuid,
    /// `slack` or `telegram`.
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    /// Slack's workspace, or the Telegram bot's username.
    pub workspace_name: String,
    /// A link may be made: the gateway is enabled and the person may talk
    /// with the agent. Otherwise their identity stays theirs to remove.
    pub available: bool,
    pub linked: Option<GatewayIdentity>,
    /// A Telegram link under way: a code asked for, unexpired.
    pub pending: Option<GatewayLinkPending>,
}

/// A Telegram link under way (ADR-133).
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayLinkPending {
    pub expires_at: String,
    /// How Telegram showed whoever accepted the link, to confirm; `None`
    /// until then. A name anyone chooses never starts with `@`, which only
    /// a Telegram username does.
    pub claimed_by: Option<String>,
    /// Their Telegram identifier, which no one chooses.
    pub claimed_id: Option<String>,
}

/// `POST /account/gateways/{id}/telegram-code`: the bot's link carrying a
/// new code, to open in Telegram, and until when (ADR-133).
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TelegramLink {
    pub link: String,
    pub expires_at: String,
}

/// Who the person is on the gateway's service.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayIdentity {
    /// Their Slack user, or their Telegram identifier.
    pub external_id: String,
    /// The address Slack gave as theirs when they linked it.
    pub address: Option<String>,
    /// The name Telegram showed when they linked it (ADR-133).
    pub name: Option<String>,
    pub linked_at: String,
}

/// `POST /account/gateways/link/preview`: what the link would link.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GatewayLinkPreview {
    pub gateway_id: Uuid,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub workspace_name: String,
    /// The Slack user the link names.
    pub external_id: String,
    pub expires_at: String,
}

/// Body of `POST /account/gateways/link/preview` and `POST
/// /account/gateways/link`: the code of the link Slack gave, from its
/// fragment.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayLinkCode {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 4000))]
    pub code: String,
}
