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
}

/// Body of `PUT /admin/approval-routing`: replaces the risks.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalRoutingUpdate {
    pub manager_risks: Vec<ToolRisk>,
}
