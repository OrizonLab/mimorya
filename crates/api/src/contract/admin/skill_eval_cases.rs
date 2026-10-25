//! The evaluation cases of a skill (ADR-103): as the skill's detail lists
//! them, and as the administration writes one.

use mimorya_db::repositories::skill_eval_cases::EvalCaseRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::http::{iso, trimmed};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkillEvalCaseView {
    pub id: Uuid,
    pub name: String,
    /// What the case asks, as a person would.
    pub request: String,
    /// What a good answer does, one criterion each.
    pub criteria: Vec<String>,
    pub created_by: Option<String>,
    pub updated_at: String,
}

impl SkillEvalCaseView {
    pub fn from_row(c: &EvalCaseRow) -> Self {
        Self {
            id: c.id,
            name: c.name.clone(),
            request: c.request.clone(),
            criteria: c.criteria.clone(),
            created_by: c.created_by.clone(),
            updated_at: iso(&c.updated_at),
        }
    }
}

/// Body of `POST /admin/skills/{id}/cases` and of
/// `PUT /admin/skill-cases/{id}`: the case as a whole.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillEvalCaseInput {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    #[validate(length(min = 1, max = 4000))]
    pub request: String,
    /// Blank ones are left out; 1 to 8 remain, of 300 characters at most.
    #[validate(length(min = 1, max = 8))]
    pub criteria: Vec<String>,
}
