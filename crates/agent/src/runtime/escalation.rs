//! A run's model escalation (ADR-207): the stronger model of the company's
//! provider, called while the run is escalated, within the company's
//! monthly budget and the escalations a run may have. Past either bound, or
//! should a call to it fail, the run goes on with its main model, and the
//! event is journaled once. A run escalates after a failed review, once,
//! from where it stands: one message is appended, nothing done is done
//! again; or, while the company's router is on, from its start (by the
//! company's rule or a classification, `routing`) or once it loads a skill
//! that asks for the escalation model. Its main model may also consult it
//! as Anthropic's advisor (`advisor`).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use super::ports::EngineAuditEntry;
use super::state::RunCheckpoint;
use super::types::RuntimeDeps;
use super::verification::correction_message;
use crate::llm::{EscalationRoute, LlmProvider, ModelRole, TaskClass};

/// The company's escalation, as its runs use it.
#[derive(Clone)]
pub struct Escalation {
    pub provider: Arc<dyn LlmProvider>,
    /// Whether a failed review escalates the run (step 1).
    pub after_failure: bool,
    /// While the router is on (step 2), its rule for this kind of run: the
    /// model the run starts with. A skill loaded may then switch the run.
    pub router: Option<EscalationRoute>,
    /// The model the main model may consult as Anthropic's advisor (step 3,
    /// `advisor`): the escalation model, as the company chose it.
    pub advisor: Option<String>,
    pub max_per_run: u32,
    pub budget: Arc<dyn EscalationBudget>,
}

/// What the company's escalations may still spend this month.
#[async_trait]
pub trait EscalationBudget: Send + Sync {
    /// Whether they have spent less than the month's budget.
    async fn allows(&self) -> anyhow::Result<bool>;
}

/// The bound that stopped an escalation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Bound {
    MonthlyBudget,
    PerRun,
}

impl Bound {
    fn reason(self) -> &'static str {
        match self {
            Bound::MonthlyBudget => "monthly_budget",
            Bound::PerRun => "per_run",
        }
    }
}

/// Why a run escalated, as journaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reason {
    AfterFailure,
    /// The company's rule for its kind of run.
    Rule,
    /// The main model's classification of its request.
    Classification,
    /// A skill it loaded asks for the escalation model.
    Skill,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Reason::AfterFailure => "after_failure",
            Reason::Rule => "rule",
            Reason::Classification => "classification",
            Reason::Skill => "skill",
        }
    }
}

/// The provider of the run's next model call, and the role it plays: the
/// escalation model while the run is escalated and within its bounds, else
/// the main one. A run escalated whose company no longer has an escalation
/// goes back to its main model.
pub(super) async fn next_model(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
) -> anyhow::Result<(Arc<dyn LlmProvider>, ModelRole)> {
    let main = deps.router.provider_for(TaskClass::Agent);
    let Some(escalation) = &deps.escalation else {
        if state.escalation.escalated {
            tracing::warn!("Run escalated without an escalation any more: back to the main model");
            state.escalation.escalated = false;
        }
        return Ok((main, ModelRole::Main));
    };
    // A skill loaded since the last call asks for the escalation model:
    // heeded once a run, while the router is on; after the advisor's
    // consultation still to run, which the main model's call gives it.
    if state.escalation.skill_asked
        && !state.escalation.skill_heeded
        && !state.escalation.advisor_pending
    {
        state.escalation.skill_heeded = true;
        if escalation.router.is_some() {
            start_escalation(deps, state, Reason::Skill).await?;
        }
    }
    if !state.escalation.escalated {
        return Ok((main, ModelRole::Main));
    }
    if let Some(bound) = bound_reached(escalation, state).await? {
        state.escalation.escalated = false;
        limited(deps, state, bound).await?;
        return Ok((main, ModelRole::Main));
    }
    state.escalation.calls += 1;
    Ok((escalation.provider.clone(), ModelRole::Escalation))
}

/// A call to the escalation model failed: the run goes on with its main
/// model, and the failure is journaled, without its content.
pub(super) async fn escalation_failed(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
    error: &str,
) -> anyhow::Result<()> {
    tracing::warn!(error = %error, "Escalation model call failed: back to the main model");
    state.escalation.escalated = false;
    journal(deps, "model_escalation.failed", "failure", None).await
}

/// After a review the main model's corrections did not pass: whether the
/// run escalates, once, within its bounds. It then goes on from where it
/// stands with the escalation model, told what the review found.
pub(super) async fn escalate_after_failure(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
    issues: &[String],
) -> anyhow::Result<bool> {
    let Some(escalation) = &deps.escalation else {
        return Ok(false);
    };
    if !escalation.after_failure
        || state.escalation.after_failure_used
        || state.escalation.escalated
    {
        return Ok(false);
    }
    state.escalation.after_failure_used = true;
    if !start_escalation(deps, state, Reason::AfterFailure).await? {
        return Ok(false);
    }
    state
        .messages
        .push(crate::llm::LlmMessage::user(escalation_message(issues)));
    Ok(true)
}

/// Escalates the run for `reason`, within its bounds: whether it did. A
/// run already escalated stays so.
pub(super) async fn start_escalation(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
    reason: Reason,
) -> anyhow::Result<bool> {
    let Some(escalation) = &deps.escalation else {
        return Ok(false);
    };
    if state.escalation.escalated {
        return Ok(false);
    }
    if let Some(bound) = bound_reached(escalation, state).await? {
        limited(deps, state, bound).await?;
        return Ok(false);
    }
    state.escalation.escalated = true;
    journal(
        deps,
        "model_escalation.started",
        "success",
        Some(reason.as_str()),
    )
    .await?;
    Ok(true)
}

/// Whether the month's budget is spent, said once a run if so: then no
/// call counted in it is made, a classification included.
pub(super) async fn budget_spent(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
) -> anyhow::Result<bool> {
    let Some(escalation) = &deps.escalation else {
        return Ok(true);
    };
    if escalation.budget.allows().await? {
        return Ok(false);
    }
    limited(deps, state, Bound::MonthlyBudget).await?;
    Ok(true)
}

pub(super) async fn bound_reached(
    escalation: &Escalation,
    state: &RunCheckpoint,
) -> anyhow::Result<Option<Bound>> {
    if state.escalation.calls >= escalation.max_per_run {
        return Ok(Some(Bound::PerRun));
    }
    if !escalation.budget.allows().await? {
        return Ok(Some(Bound::MonthlyBudget));
    }
    Ok(None)
}

/// A bound stopped an escalation: journaled once a run.
pub(super) async fn limited(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
    bound: Bound,
) -> anyhow::Result<()> {
    if state.escalation.limited {
        return Ok(());
    }
    state.escalation.limited = true;
    journal(
        deps,
        "model_escalation.limited",
        "denied",
        Some(bound.reason()),
    )
    .await
}

async fn journal(
    deps: &RuntimeDeps,
    action: &str,
    outcome: &'static str,
    reason: Option<&str>,
) -> anyhow::Result<()> {
    let model = deps
        .escalation
        .as_ref()
        .map(|escalation| escalation.provider.model().to_string());
    deps.ports
        .audit
        .record(EngineAuditEntry {
            action: action.into(),
            outcome,
            resource_type: Some("model".into()),
            resource_id: model,
            reason: reason.map(str::to_string),
            metadata: json!({}),
        })
        .await
}

/// The platform's message that hands the run to the escalation model: what
/// the review found, and that what was done is done.
pub fn escalation_message(issues: &[String]) -> String {
    let mut message = correction_message(issues);
    message.push_str(
        "\nLes actions déjà exécutées dans cette conversation l’ont été : ne les refais pas. \
         Reprends à partir des résultats obtenus.",
    );
    message
}
