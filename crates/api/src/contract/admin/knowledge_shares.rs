//! A company's units' knowledge shared with other units (ADR-143), as its
//! administrators of access policies (`policy:manage`, company-wide) list,
//! make and remove them.

use chrono::{DateTime, Utc};
use mimorya_db::repositories::knowledge_shares::KnowledgeShareRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::Sensitivity;

/// A unit's knowledge shared with another.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeShareView {
    pub id: Uuid,
    /// The unit whose space is shared.
    pub source_org_unit_id: Uuid,
    /// The unit whose members read it.
    pub target_org_unit_id: Uuid,
    /// Whether the members of the units under the target read it too.
    pub include_descendants: bool,
    /// The memory filed in the shared unit's space.
    pub memory: bool,
    /// The documents deposited there.
    pub documents: bool,
    /// The highest sensitivity read through it.
    pub max_sensitivity: Sensitivity,
    /// ISO 8601; absent, until removed.
    pub expires_at: Option<String>,
    pub created_at: String,
}

impl KnowledgeShareView {
    /// `None` for a sensitivity the base does not hold.
    pub fn from_row(row: &KnowledgeShareRow) -> Option<Self> {
        let share = row.share()?;
        Some(Self {
            id: row.id,
            source_org_unit_id: row.source_org_unit_id,
            target_org_unit_id: row.target_org_unit_id,
            include_descendants: row.include_descendants,
            memory: row.shares_memory,
            documents: row.shares_documents,
            max_sensitivity: share.ceiling.into(),
            expires_at: row.expires_at.map(|at| at.to_rfc3339()),
            created_at: row.created_at.to_rfc3339(),
        })
    }
}

/// A unit a share may name.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ShareableUnit {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
}

/// `GET /admin/knowledge-shares`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeShares {
    pub shares: Vec<KnowledgeShareView>,
    /// The company's units, each with its parent.
    pub units: Vec<ShareableUnit>,
}

/// Body of `POST /admin/knowledge-shares`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewKnowledgeShare {
    pub source_org_unit_id: Uuid,
    pub target_org_unit_id: Uuid,
    #[serde(default)]
    #[ts(optional)]
    pub include_descendants: Option<bool>,
    pub memory: bool,
    pub documents: bool,
    pub max_sensitivity: Sensitivity,
    /// ISO 8601; absent, until removed.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub expires_at: Option<DateTime<Utc>>,
}
