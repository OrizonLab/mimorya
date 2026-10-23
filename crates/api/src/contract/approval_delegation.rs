//! Absences declared by whoever decides requests as a manager, or for them
//! by an account administrator (ADR-078), with whom they name to stand in
//! for them, and those the reader stands in for (ADR-071).

use chrono::{DateTime, Utc};
use mimorya_db::repositories::approval_delegations::ApprovalDelegation;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::approvals::ApprovalPerson;
use crate::http::iso;

/// `GET /approval-delegation`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDelegations {
    /// The reader's absence, declared and not ended: who stands in for them.
    pub own: Option<Delegation>,
    /// The absences the reader covers, under way or to come.
    pub standing_in_for: Vec<Delegation>,
    /// Whom the reader may name, by name: active colleagues holding
    /// `approval:decide` on a unit or the company (ADR-073). Empty unless
    /// the reader holds it too.
    pub candidates: Vec<ApprovalPerson>,
}

/// An absence: from when until when the delegate stands in for the absent
/// person, deciding what they hold `approval:decide` for.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Delegation {
    pub absent: ApprovalPerson,
    pub delegate: ApprovalPerson,
    pub starts_at: String,
    pub ends_at: String,
    /// The administrator who declared it on the absent person's behalf
    /// (ADR-078); `null` when they declared it themselves.
    pub declared_by: Option<ApprovalPerson>,
}

impl From<&ApprovalDelegation> for Delegation {
    fn from(d: &ApprovalDelegation) -> Self {
        Self {
            absent: ApprovalPerson {
                id: d.user_id,
                display_name: d.user_name.clone(),
            },
            delegate: ApprovalPerson {
                id: d.delegate_user_id,
                display_name: d.delegate_name.clone(),
            },
            starts_at: iso(&d.starts_at),
            ends_at: iso(&d.ends_at),
            declared_by: d
                .declared_by_user_id
                .zip(d.declared_by_name.clone())
                .map(|(id, display_name)| ApprovalPerson { id, display_name }),
        }
    }
}

/// Body of `PUT /approval-delegation`: declares, or replaces, the reader's
/// absence.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Absence {
    /// The colleague who stands in.
    pub delegate_id: Uuid,
    /// ISO 8601; absent, from now.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub starts_at: Option<DateTime<Utc>>,
    /// ISO 8601, at most 90 days after the start.
    #[ts(type = "string")]
    pub ends_at: DateTime<Utc>,
}
