//! The accounts each person links at their company's providers (ADR-116):
//! where they may link one, what is linked, and where the browser goes to
//! grant access.

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

/// `GET /account/links`.
#[derive(Debug, Serialize, TS)]
pub struct AccountLinks {
    pub connectors: Vec<AccountLinkView>,
}

/// A connector the person may link an account at, or has one at.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountLinkView {
    pub connector_id: Uuid,
    pub name: String,
    /// `microsoft365`.
    #[serde(rename = "type")]
    pub connector_type: String,
    /// A link may start: the connector is enabled and the person may use
    /// the company's connectors. Otherwise their link stays theirs to remove.
    pub available: bool,
    /// Its tools may send e-mail and create events, each validated: a
    /// link asks those permissions too (ADR-119, ADR-120).
    pub write_tools: bool,
    pub linked: Option<LinkedAccount>,
}

/// The account linked: never its token.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LinkedAccount {
    /// As the provider's token said it.
    pub address: String,
    /// The permissions the person granted, as the provider listed them.
    pub scopes: Vec<String>,
    /// Those the application asks now that the person did not grant: the
    /// tools that need them say so until the person links again (ADR-119).
    pub to_grant: Vec<String>,
    pub linked_at: String,
}

/// `POST /account/links/{connectorId}`: where the browser goes to sign in
/// and grant access.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountLinkStart {
    pub redirect_url: String,
}
