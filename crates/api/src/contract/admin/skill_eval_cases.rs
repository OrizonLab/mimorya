//! The evaluation cases of a skill (ADR-103): as the skill's detail lists
//! them, and as the administration writes one.

use mimorya_db::repositories::skill_eval_cases::EvalCaseRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use mimorya_core::skill_eval_cases::{MAX_NAME_CHARS, MAX_REQUEST_CHARS, MIN_NAME_CHARS};

use crate::http::{iso, trimmed};

/// Lines of criteria a case may send, blank ones included: a text area cut
/// at its line breaks.
const MAX_CRITERIA_LINES: u64 = 50;
/// Characters of a request as sent, its blanks around included; once
/// trimmed, `MAX_REQUEST_CHARS` remain at most (`case_problem`).
const MAX_REQUEST_SENT: u64 = 2 * MAX_REQUEST_CHARS;

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
    #[validate(length(min = MIN_NAME_CHARS, max = MAX_NAME_CHARS))]
    pub name: String,
    #[validate(length(max = MAX_REQUEST_SENT))]
    pub request: String,
    /// One per line sent, at most `MAX_CRITERIA_LINES`; blank ones are left
    /// out, and 1 to 8 must remain, of 300 characters at most
    /// (`skill_eval_cases::case_problem`).
    #[validate(length(max = MAX_CRITERIA_LINES))]
    pub criteria: Vec<String>,
}
