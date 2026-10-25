//! What the administration reads of a skill's evaluations (ADR-104): each
//! one, its results, and what its cases add up to (`summarize`). Who reads
//! the answers is the contract's to say (`contract/admin`).

use mimorya_core::skill_evaluations::{CaseOutcome, CaseResults, Summary, summarize};
use mimorya_db::TenantTx;
use mimorya_db::repositories::skill_evaluations::{
    EvaluationRow, ResultRow, get_evaluation, last_completed_evaluation, list_evaluations,
    results_of,
};
use uuid::Uuid;

/// An evaluation, its results and what they add up to.
pub struct EvaluationRead {
    pub row: EvaluationRow,
    pub results: Vec<ResultRow>,
    /// Case by case, in the order of the cases.
    pub cases: Vec<CaseResults>,
    pub summary: Summary,
}

/// Each of `cases` cases' outcomes, from its results.
pub fn case_results(cases: usize, results: &[ResultRow]) -> Vec<CaseResults> {
    let mut outcomes = vec![
        CaseResults {
            evaluated: None,
            reference: None,
        };
        cases
    ];
    for result in results {
        let Some(case) = usize::try_from(result.position)
            .ok()
            .and_then(|position| outcomes.get_mut(position))
        else {
            continue;
        };
        let outcome = CaseOutcome::parse(&result.outcome);
        if result.reference {
            case.reference = outcome;
        } else {
            case.evaluated = outcome;
        }
    }
    outcomes
}

/// The skill's evaluations, the latest first.
pub async fn evaluations_of(
    tx: &mut TenantTx,
    skill_id: Uuid,
) -> anyhow::Result<Vec<EvaluationRead>> {
    let rows = list_evaluations(tx, skill_id).await?;
    read(tx, rows).await
}

pub async fn evaluation(tx: &mut TenantTx, id: Uuid) -> anyhow::Result<Option<EvaluationRead>> {
    let rows: Vec<EvaluationRow> = get_evaluation(tx, id).await?.into_iter().collect();
    Ok(read(tx, rows).await?.pop())
}

/// The last completed evaluation of the skill's `version`.
pub async fn last_evaluation(
    tx: &mut TenantTx,
    skill_id: Uuid,
    version: i32,
) -> anyhow::Result<Option<EvaluationRead>> {
    let rows: Vec<EvaluationRow> = last_completed_evaluation(tx, skill_id, version)
        .await?
        .into_iter()
        .collect();
    Ok(read(tx, rows).await?.pop())
}

async fn read(tx: &mut TenantTx, rows: Vec<EvaluationRow>) -> anyhow::Result<Vec<EvaluationRead>> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut results = results_of(tx, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let (own, others): (Vec<_>, Vec<_>) =
                results.drain(..).partition(|r| r.evaluation_id == row.id);
            results = others;
            let count = row.cases.as_array().map_or(0, Vec::len);
            let cases = case_results(count, &own);
            let summary = summarize(&cases);
            EvaluationRead {
                row,
                results: own,
                cases,
                summary,
            }
        })
        .collect())
}
