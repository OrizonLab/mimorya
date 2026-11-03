//! Allow policies proposed by one administrator of the access policies
//! (`policy:manage`, company-wide) and decided by another (ADR-168): the
//! listing, and a proposal as written.

use chrono::{DateTime, Utc};
use mimorya_core::authz::Condition;
use mimorya_db::repositories::policy_proposals::ProposalRow;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::contract::stored_text;
use crate::http::{iso, trimmed};

/// Where a proposal stands: a pending one past its expiry is `expired`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected,
    Withdrawn,
    Expired,
}

/// An administrator who proposed or decided.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProposalPerson {
    pub id: Uuid,
    pub display_name: String,
}

/// An allow policy as proposed: a new one, or the change of `policyId`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PolicyProposalView {
    pub id: Uuid,
    /// The allow policy it changes; absent for a new one.
    pub policy_id: Option<Uuid>,
    /// The policy as it will stand once approved.
    pub description: String,
    pub permissions: Vec<String>,
    #[ts(type = "Record<string, unknown>")]
    pub condition: Value,
    pub enabled: bool,
    pub proposed_by: ProposalPerson,
    /// Proposed by the administrator reading: they may withdraw it, never
    /// approve or reject it.
    pub mine: bool,
    pub proposed_at: String,
    /// Past it, nobody approves it.
    pub expires_at: String,
    pub status: ProposalStatus,
    /// Pending, its policy changed since: it can no longer be approved.
    pub outdated: bool,
    /// Absent while pending, or once their account is gone.
    pub decided_by: Option<ProposalPerson>,
    pub decided_at: Option<String>,
}

impl PolicyProposalView {
    pub fn from_row(row: &ProposalRow, reader: Uuid, now: DateTime<Utc>) -> anyhow::Result<Self> {
        let mut status = stored_text("policy proposal status", &row.status)?;
        if status == ProposalStatus::Pending && row.expires_at <= now {
            status = ProposalStatus::Expired;
        }
        let decided_by = row
            .decided_by_user_id
            .zip(row.decided_by_name.clone())
            .map(|(id, display_name)| ProposalPerson { id, display_name });
        Ok(Self {
            id: row.id,
            policy_id: row.policy_id,
            description: row.description.clone(),
            permissions: row.permissions.clone(),
            condition: row.condition.clone(),
            enabled: row.enabled,
            proposed_by: ProposalPerson {
                id: row.proposed_by_user_id,
                display_name: row.proposed_by_name.clone(),
            },
            mine: row.proposed_by_user_id == reader,
            proposed_at: iso(&row.proposed_at),
            expires_at: iso(&row.expires_at),
            status,
            outdated: row.outdated,
            decided_by,
            decided_at: row.decided_at.as_ref().map(iso),
        })
    }
}

/// `GET /admin/policy-proposals`: every pending proposal, then the latest
/// decided.
#[derive(Debug, Serialize, TS)]
pub struct PolicyProposals {
    pub proposals: Vec<PolicyProposalView>,
}

/// Body of `POST /admin/policy-proposals`: the allow policy as it will
/// stand, a new one or `policyId` changed.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewPolicyProposal {
    #[serde(default)]
    #[ts(optional)]
    pub policy_id: Option<Uuid>,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 3, max = 300))]
    pub description: String,
    #[validate(length(max = 50))]
    pub permissions: Vec<String>,
    /// `{ all }`, `{ any }`, `{ not }` or a comparison, as `PolicyView.condition`.
    #[ts(type = "Record<string, unknown>")]
    pub condition: Condition,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
}
