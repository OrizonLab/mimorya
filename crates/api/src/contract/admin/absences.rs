//! A collaborator's absence as an account administrator declares it for
//! them (ADR-078): the same as their own (ADR-071), with whom they may name.

use serde::Serialize;
use ts_rs::TS;

use crate::contract::{ApprovalPerson, Delegation};

/// `GET /admin/users/{id}/absence`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CollaboratorAbsence {
    /// Declared and not ended, by the collaborator or an administrator.
    pub absence: Option<Delegation>,
    /// Whom the administrator may name: the collaborator's active
    /// colleagues holding `approval:decide` on a unit or the company.
    pub candidates: Vec<ApprovalPerson>,
}

/// Answer of `PUT` (body `Absence`) and `DELETE /admin/users/{id}/absence`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceRecorded {
    /// Whether the collaborator was told by e-mail: not without a relay, nor
    /// when sending failed.
    pub notified: bool,
}
