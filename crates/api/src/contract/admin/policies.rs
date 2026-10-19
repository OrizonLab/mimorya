//! Access policies (`policy:manage`): every policy, and what a deny policy
//! may restrict and read (ADR-022).

use mimorya_db::repositories::access_policies::PolicyRow;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::contract::stored_text;
use crate::http::iso;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PolicyView {
    pub id: Uuid,
    pub description: String,
    pub effect: PolicyEffect,
    pub permissions: Vec<String>,
    /// The condition as stored: `{ all }`, `{ any }`, `{ not }` or a comparison.
    #[ts(type = "Record<string, unknown>")]
    pub condition: Value,
    pub enabled: bool,
    /// Only deny policies are written from the administration (ADR-022).
    pub editable: bool,
    pub updated_at: String,
}

impl PolicyView {
    pub fn from_row(p: &PolicyRow) -> anyhow::Result<Self> {
        let effect = stored_text("policy effect", &p.effect)?;
        Ok(Self {
            id: p.id,
            description: p.description.clone(),
            effect,
            permissions: p.permissions.clone(),
            condition: p.condition.clone(),
            enabled: p.enabled,
            editable: effect == PolicyEffect::Deny,
            updated_at: iso(&p.updated_at),
        })
    }
}

/// A permission a policy may restrict: a business one the platform checks.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RestrictablePermission {
    pub permission: String,
    pub label: String,
    /// Checked resource by resource: a condition on the resource applies.
    pub per_resource: bool,
}

/// An attribute a condition can read.
#[derive(Debug, Serialize, TS)]
pub struct ConditionAttribute {
    /// e.g. `subject.attributes.contractType`.
    pub path: String,
    pub label: String,
}

/// `GET /admin/policies`.
#[derive(Debug, Serialize, TS)]
pub struct Policies {
    pub policies: Vec<PolicyView>,
    pub permissions: Vec<RestrictablePermission>,
    /// Domains a policy may restrict as a whole (`domain:*`).
    pub domains: Vec<String>,
    pub attributes: Vec<ConditionAttribute>,
}
