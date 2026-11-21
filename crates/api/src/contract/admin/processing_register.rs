//! The company's record of processing activities (RGPD art. 30, ADR-217),
//! as who holds `compliance:read` company-wide reads it: drawn from its
//! settings when asked, in French, with the keys of what a screen sorts or
//! marks.

use serde::Serialize;
use ts_rs::TS;

/// Where a recipient stands, as far as the platform knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RecipientPlaceView {
    EuropeanUnion,
    /// A group established outside the European Union: a transfer to frame.
    OutsideEuropeanUnion,
    ListedByOperator,
    DeclaredByCompany,
    Operator,
    Unknown,
}

/// Who chose a recipient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RecipientChoiceView {
    Company,
    /// The operator, who supplies it: one of its own processors.
    Operator,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRecipient {
    pub name: String,
    /// What it receives the data for.
    pub role: String,
    pub place: RecipientPlaceView,
    pub place_label: String,
    pub chosen_by: RecipientChoiceView,
    pub chosen_by_label: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RegisterActivity {
    /// `accounts`, `assistance`, `memory`, `documents`, `connectors`,
    /// `gateways`, `audit` or `usage`.
    pub key: String,
    pub name: String,
    pub purpose: String,
    pub subjects: Vec<String>,
    pub data: Vec<String>,
    /// What it says of sensitive data, if anything.
    pub sensitive: Option<String>,
    pub recipients: Vec<RegisterRecipient>,
    pub retention: Vec<String>,
    pub notes: Vec<String>,
}

/// `GET /admin/processing-register`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingRegisterView {
    /// The controller: the company's name.
    pub controller: String,
    /// When it was drawn (RFC 3339): it states the settings of that moment.
    pub drawn_at: String,
    pub activities: Vec<RegisterActivity>,
    /// A general description of the security measures.
    pub measures: Vec<String>,
    /// What the company, controller, completes itself.
    pub to_complete: Vec<String>,
}
