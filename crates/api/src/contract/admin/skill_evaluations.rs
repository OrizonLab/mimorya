//! The evaluations of a skill's versions (ADR-104): one asked for, the
//! skill's listed with what their cases add up to, and one read case by
//! case. The answers, the judge's issues and the tools called are shown to
//! whom the evaluation ran for only (`skill_evaluations::shown_to`): they
//! were read with their rights.

use mimorya_db::repositories::skill_evaluations::ResultRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use crate::contract::stored::{stored, stored_text};
use crate::http::iso;
use crate::services::skill_evaluations::{EvaluationRead, StoredCase};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    /// Waiting for a process that executes runs.
    Queued,
    Running,
    Completed,
    /// Ended before its last case: why, in `failure`.
    Failed,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkillEvaluationView {
    pub id: Uuid,
    pub version: i32,
    /// The active version played beside it, when another one was evaluated.
    pub reference_version: Option<i32>,
    pub status: EvaluationStatus,
    pub failure: Option<String>,
    pub cases: usize,
    /// Cases played so far for the version evaluated.
    pub played: usize,
    pub passed: usize,
    pub reference_passed: Option<usize>,
    /// Cases the active version passed and the evaluated one did not.
    pub regressions: usize,
    pub launched_by: Option<String>,
    pub cost_micros: i64,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

impl SkillEvaluationView {
    pub fn from_read(e: &EvaluationRead) -> anyhow::Result<Self> {
        let row = &e.row;
        Ok(Self {
            id: row.id,
            version: row.version,
            reference_version: row.reference_version,
            status: stored_text("evaluation status", &row.status)?,
            failure: row.failure.clone(),
            cases: e.cases.len(),
            played: e.cases.iter().filter(|c| c.evaluated.is_some()).count(),
            passed: e.summary.passed,
            reference_passed: row.reference_version.map(|_| e.summary.reference_passed),
            regressions: e.summary.regressions,
            launched_by: row.launched_by.clone(),
            cost_micros: row.cost_micros,
            created_at: iso(&row.created_at),
            started_at: row.started_at.as_ref().map(iso),
            finished_at: row.finished_at.as_ref().map(iso),
        })
    }
}

/// `GET /admin/skills/{id}/evaluations`: the latest first.
#[derive(Debug, Serialize, TS)]
pub struct SkillEvaluations {
    pub evaluations: Vec<SkillEvaluationView>,
}

/// As `skill_evaluations::CaseOutcome` stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CaseOutcomeView {
    Passed,
    Failed,
    /// No answer to judge, or no verdict.
    Error,
}

/// As the evaluation's `ToolTrace` stores it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallView {
    pub name: String,
    pub summary: String,
    pub is_error: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CaseResultView {
    pub outcome: CaseOutcomeView,
    /// Whether the agent loaded the version played.
    pub skill_loaded: bool,
    /// Why no answer was judged, in the platform's words.
    pub error: Option<String>,
    /// Shown to whom the evaluation ran for only, as the next two.
    pub answer: Option<String>,
    pub issues: Option<Vec<String>>,
    pub tools: Option<Vec<ToolCallView>>,
}

impl CaseResultView {
    fn from_row(r: &ResultRow, shown: bool) -> anyhow::Result<Self> {
        let tools: Vec<ToolCallView> = stored("evaluation tools", &r.tools)?;
        Ok(Self {
            outcome: stored_text("case outcome", &r.outcome)?,
            skill_loaded: r.skill_loaded,
            error: r.error.clone(),
            answer: r.answer.clone().filter(|_| shown),
            issues: shown.then(|| r.issues.clone()),
            tools: shown.then_some(tools),
        })
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedCaseView {
    pub name: String,
    pub request: String,
    pub criteria: Vec<String>,
    /// For the version evaluated; `None` while not played.
    pub result: Option<CaseResultView>,
    /// For the active version played beside it.
    pub reference: Option<CaseResultView>,
    pub regression: bool,
}

/// `GET /admin/skill-evaluations/{id}`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkillEvaluationDetail {
    pub evaluation: SkillEvaluationView,
    pub cases: Vec<EvaluatedCaseView>,
    /// Whether the answers, issues and tools are shown: to whom the
    /// evaluation ran for.
    pub details_shown: bool,
}

impl SkillEvaluationDetail {
    /// The answers, issues and tools `shown` or not (`shown_to`).
    pub fn new(e: &EvaluationRead, shown: bool) -> anyhow::Result<Self> {
        let stored: Vec<StoredCase> = serde_json::from_value(e.row.cases.clone())?;
        let result_of = |position: usize, reference: bool| {
            e.results
                .iter()
                .find(|r| r.reference == reference && usize::try_from(r.position) == Ok(position))
                .map(|r| CaseResultView::from_row(r, shown))
                .transpose()
        };
        let mut cases = Vec::with_capacity(stored.len());
        for (position, case) in stored.into_iter().enumerate() {
            cases.push(EvaluatedCaseView {
                name: case.name,
                request: case.request,
                criteria: case.criteria,
                result: result_of(position, false)?,
                reference: result_of(position, true)?,
                regression: e.cases.get(position).is_some_and(|c| c.regressed()),
            });
        }
        Ok(Self {
            evaluation: SkillEvaluationView::from_read(e)?,
            cases,
            details_shown: shown,
        })
    }
}

/// Body of `POST /admin/skills/{id}/evaluations`: the version tried.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationRequest {
    #[validate(range(min = 1))]
    pub version: i32,
}
