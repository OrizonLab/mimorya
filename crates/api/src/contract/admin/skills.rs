//! The company's skills (`skill:manage`, ADR-052): the list, a skill with
//! its versions, and what the administration writes: a skill, a new version,
//! a description, the version made active, a proposal of the agent's turned
//! down (ADR-100). Its evaluation cases: `skill_eval_cases.rs` (ADR-103).

use mimorya_db::repositories::skills::{SkillRow, SkillVersionRow};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::SkillEvalCaseView;
use crate::contract::stored::stored_text;
use crate::http::{iso, trimmed, trimmed_opt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    /// Never active yet.
    Draft,
    Active,
    Retired,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkillView {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: SkillStatus,
    pub active_version: Option<i32>,
    pub latest_version: Option<i32>,
    /// The agent's proposals awaiting a decision (ADR-100).
    pub proposals: i64,
    pub updated_at: String,
}

impl SkillView {
    pub fn from_row(s: &SkillRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: s.id,
            name: s.name.clone(),
            description: s.description.clone(),
            status: stored_text("skill status", &s.status)?,
            active_version: s.active_version,
            latest_version: s.latest_version,
            proposals: s.proposals,
            updated_at: iso(&s.updated_at),
        })
    }
}

/// `GET /admin/skills`.
#[derive(Debug, Serialize, TS)]
pub struct SkillList {
    pub skills: Vec<SkillView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SkillVersionStatus {
    /// Written, never active.
    Draft,
    /// Proposed by the agent, awaiting the administrators (ADR-100).
    Candidate,
    Active,
    /// A proposal the administrators turned down (ADR-100).
    Rejected,
    /// Active before another version, or before the skill was retired.
    Retired,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersionView {
    pub version: i32,
    pub status: SkillVersionStatus,
    /// The procedure handed to the agent, in Markdown.
    pub content: String,
    pub changelog: String,
    /// Proposed by the agent for its author, who validated it (ADR-100).
    pub proposed: bool,
    /// What a proposal of a new skill said the skill covers.
    pub proposed_description: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    /// Who made it active last, and when.
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
}

impl SkillVersionView {
    pub fn from_row(v: &SkillVersionRow) -> anyhow::Result<Self> {
        Ok(Self {
            version: v.version,
            status: stored_text("skill version status", &v.status)?,
            content: v.content.clone(),
            changelog: v.changelog.clone(),
            proposed: v.proposed,
            proposed_description: v.proposed_description.clone(),
            created_by: v.created_by.clone(),
            created_at: iso(&v.created_at),
            approved_by: v.approved_by.clone(),
            approved_at: v.approved_at.as_ref().map(iso),
        })
    }
}

/// `GET /admin/skills/{id}`: the skill and its versions, the latest first,
/// and its evaluation cases in the order they were written (ADR-103).
#[derive(Debug, Serialize, TS)]
pub struct SkillDetail {
    pub skill: SkillView,
    pub versions: Vec<SkillVersionView>,
    pub cases: Vec<SkillEvalCaseView>,
}

/// `POST /admin/skills/{id}/versions`.
#[derive(Debug, Serialize, TS)]
pub struct SkillVersionCreated {
    pub version: i32,
}

/// Body of `POST /admin/skills`: a skill and its first version, a draft.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewSkill {
    #[serde(deserialize_with = "trimmed")]
    pub name: String,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 10, max = 300))]
    pub description: String,
    #[validate(length(max = 20000))]
    pub content: String,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 300))]
    #[ts(optional)]
    pub changelog: Option<String>,
}

/// Body of `PATCH /admin/skills/{id}`.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillUpdate {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 10, max = 300))]
    pub description: String,
}

/// Body of `POST /admin/skills/{id}/versions`: a new draft version.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewSkillVersion {
    #[validate(length(max = 20000))]
    pub content: String,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(max = 300))]
    #[ts(optional)]
    pub changelog: Option<String>,
}

/// Body of `PUT /admin/skills/{id}/activation`: the version made active.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillActivation {
    #[validate(range(min = 1))]
    pub version: i32,
    /// Made active against the regressions its last evaluation found
    /// (ADR-104). Absent: not confirmed.
    #[serde(default)]
    #[ts(optional)]
    pub confirmed: Option<bool>,
}

/// Body of `POST /admin/skills/{id}/rejection`: the proposal turned down.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillRejection {
    #[validate(range(min = 1))]
    pub version: i32,
}
