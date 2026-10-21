//! The risks whose actions a company has its managers validate (ADR-059),
//! read and set by those who manage its access policies.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::contract::ToolRisk;

/// `GET /admin/approval-routing`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRoutingView {
    /// Empty: whom the agent acts for validates every action, as before.
    pub manager_risks: Vec<ToolRisk>,
    /// Whether a manager is e-mailed when a request awaits them: only when
    /// the operator configured a mail relay (ADR-060).
    pub notifies_by_email: bool,
}

/// Body of `PUT /admin/approval-routing`: replaces the risks.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalRoutingUpdate {
    pub manager_risks: Vec<ToolRisk>,
}
