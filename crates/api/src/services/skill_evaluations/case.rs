//! One case played for one version (ADR-104). The agent answers the case's
//! request for whom asked for the evaluation, as in a conversation of
//! theirs, with what a sub-agent is given (ADR-098): the tools that only
//! read and run without validation, the read-only autonomy, a budget of its
//! own; told that actions are not available, it describes the one it would
//! propose. `skill_read` serves the version played. A model then judges the
//! answer on the case's criteria and on the quality criteria of the skills
//! the agent loaded, as a run's answer is (ADR-106). The result keeps the
//! answer, the issues the judge found, the tools called and whether the
//! version was loaded.

use std::sync::Arc;

use chrono::Utc;
use mimorya_agent::llm::{Effort, LlmProvider, ModelRouter};
use mimorya_agent::runtime::answer_review::review_criteria;
use mimorya_agent::runtime::verification::{JudgeInput, judge_answer};
use mimorya_agent::runtime::{
    AgentRuntime, BudgetLimits, RunConfig, RunOutcome, RunUsageTotals, RuntimeDeps, StartInput,
    UsageRecord, UsageSink, VerificationConfig, build_system_prompt,
};
use mimorya_core::authz::Actor;
use mimorya_core::autonomy::{AutonomyLevel, AutonomyPolicy};
use mimorya_core::skill_evaluations::{CaseOutcome, MAX_ANSWER_CHARS, bounded};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::access::EvaluationAccess;
use super::ports::{CasePorts, CaseScope, EvaluatedVersion, Journal, ToolTrace, case_ports};
use crate::app::Services;
use crate::services::AccessContext;
use crate::services::runs::{build_run_tools, user_profile};

/// What one case may spend at most, for one version.
pub const CASE_BUDGET: BudgetLimits = BudgetLimits {
    max_steps: 8,
    max_input_tokens: 200_000,
    max_output_tokens: 20_000,
    max_cost_micros: 500_000, // 0.50 USD
    max_duration_ms: 3 * 60_000,
};

/// The channel a case's agent acts through, as policies read it.
const CHANNEL: &str = "evaluation";

/// Told to the agent after the case's request: it may only read.
const READ_ONLY: &str = "[Message de la plateforme] Les actions qui modifient des données ne \
     sont pas disponibles pour cette demande : si elle en appelle une, décris précisément celle \
     que tu proposerais (outil et paramètres) au lieu de l’exécuter.";

const NO_VERDICT: &str = "Le contrôle de la réponse n’a pas rendu de verdict.";
const JUDGE_FAILED: &str = "Le contrôle de la réponse a échoué.";
const NO_ANSWER: &str = "L’assistant n’a pas rendu de réponse.";
const INTERNAL: &str = "Erreur interne pendant ce cas.";

/// A case as it was copied into its evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCase {
    pub name: String,
    pub request: String,
    pub criteria: Vec<String>,
}

pub struct CaseRequest<'a> {
    pub scope: CaseScope,
    pub access: &'a AccessContext,
    pub provider: Arc<dyn LlmProvider>,
    pub version: EvaluatedVersion,
    pub case: &'a StoredCase,
}

/// How the case went for the version.
pub struct Played {
    pub outcome: CaseOutcome,
    pub issues: Vec<String>,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub skill_loaded: bool,
    pub tools: Vec<ToolTrace>,
    pub usage: RunUsageTotals,
}

/// What the agent and the judge came to, before the ports are read.
struct Judged {
    outcome: CaseOutcome,
    issues: Vec<String>,
    answer: Option<String>,
    error: Option<String>,
    usage: RunUsageTotals,
}

pub async fn play_case(services: &Services, request: CaseRequest<'_>) -> Played {
    let CasePorts {
        ports,
        journal,
        knowledge,
    } = case_ports(services, request.scope, request.version.clone());
    let judged = match answer_and_judge(services, &request, ports, &journal).await {
        Ok(judged) => judged,
        Err(error) => {
            tracing::warn!(
                evaluation_id = %request.scope.evaluation_id,
                error = format!("{error:#}"),
                "Evaluation case failed"
            );
            Judged {
                outcome: CaseOutcome::Error,
                issues: Vec::new(),
                answer: None,
                error: Some(INTERNAL.into()),
                usage: RunUsageTotals::default(),
            }
        }
    };
    Played {
        outcome: judged.outcome,
        issues: judged.issues,
        answer: judged.answer,
        error: judged.error,
        skill_loaded: knowledge.loaded(),
        tools: journal.tools(),
        usage: judged.usage,
    }
}

async fn answer_and_judge(
    services: &Services,
    request: &CaseRequest<'_>,
    ports: mimorya_agent::runtime::EnginePorts,
    journal: &Journal,
) -> anyhow::Result<Judged> {
    let CaseRequest {
        scope,
        access,
        provider,
        case,
        ..
    } = request;
    let run_id = Uuid::new_v4();
    let actor = Actor::agent(Some(run_id)).with_channel(CHANNEL);
    let tools = build_run_tools(access, &actor, &ports, &services.connectors).await?;
    let skills = if access.engine.has(&access.subject, "skill:read", &actor) {
        ports.skills.list_active().await?
    } else {
        Vec::new()
    };
    let profile = user_profile(access, &services.config.assistant_name);
    let system = build_system_prompt(&profile, &skills, Utc::now());
    let deps = RuntimeDeps::new(
        ModelRouter::new(provider.clone()),
        Arc::new(EvaluationAccess {
            access: services.access.clone(),
            tenant_id: scope.tenant_id,
            user_id: scope.user_id,
        }),
        Arc::new(tools.registry.for_sub_agents()),
        ports,
        services.llm.prices().clone(),
    );
    let runtime = AgentRuntime::new(
        deps,
        RunConfig {
            run_id,
            actor,
            objective: case.request.clone(),
            autonomy: AutonomyPolicy::new(AutonomyLevel::ReadOnly),
            budget: CASE_BUDGET,
            success_criteria: Vec::new(),
            verification: VerificationConfig {
                llm_judge: false,
                // Judged below, the version's criteria among the case's.
                skill_criteria: false,
                max_correction_attempts: 1,
            },
            effort: Some(Effort::High),
            handoff: false,
        },
    );
    let outcome = runtime
        .start(StartInput {
            system,
            history: Vec::new(),
            user_turn: format!("{}\n\n{READ_ONLY}", case.request),
            unavailable_connectors: tools.unavailable_connectors,
            evidence: Vec::new(),
        })
        .await?;
    let mut usage = outcome.checkpoint().usage;
    let criteria = review_criteria(&case.criteria, &outcome.checkpoint().skill_criteria, true);
    let (answer, evidence) = match outcome {
        RunOutcome::Completed {
            answer, checkpoint, ..
        } => (answer, checkpoint.evidence),
        RunOutcome::Failed { error, .. } => return Ok(unjudged(Some(error), usage)),
        RunOutcome::AwaitingApproval { .. } | RunOutcome::Handoff { .. } => {
            return Ok(unjudged(Some(NO_ANSWER.into()), usage));
        }
    };
    let judging = JudgeInput {
        objective: &case.request,
        criteria: &criteria,
        answer: &answer,
        evidence: &evidence,
    };
    let answer = Some(bounded(&answer, MAX_ANSWER_CHARS));
    let (verdict, response) = match judge_answer(provider.as_ref(), judging).await {
        Ok(judged) => judged,
        Err(error) => {
            tracing::warn!(error = %error, "Evaluation judge failed");
            return Ok(Judged {
                outcome: CaseOutcome::Error,
                issues: Vec::new(),
                answer,
                error: Some(JUDGE_FAILED.into()),
                usage,
            });
        }
    };
    let cost = services
        .llm
        .prices()
        .cost_micros(&response.model, &response.usage);
    usage.input_tokens += response.usage.input_tokens;
    usage.output_tokens += response.usage.output_tokens;
    usage.cost_micros += cost.unwrap_or(0);
    journal
        .record(UsageRecord {
            kind: "llm",
            provider: response.provider.clone(),
            model: response.model.clone(),
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            cost_micros: cost.unwrap_or(0),
        })
        .await?;
    Ok(match verdict {
        Some(verdict) => Judged {
            outcome: if verdict.pass {
                CaseOutcome::Passed
            } else {
                CaseOutcome::Failed
            },
            issues: verdict.issues,
            answer,
            error: None,
            usage,
        },
        None => Judged {
            outcome: CaseOutcome::Error,
            issues: Vec::new(),
            answer,
            error: Some(NO_VERDICT.into()),
            usage,
        },
    })
}

/// A case that ended without an answer to judge, for `error`.
fn unjudged(error: Option<String>, usage: RunUsageTotals) -> Judged {
    Judged {
        outcome: CaseOutcome::Error,
        issues: Vec::new(),
        answer: None,
        error,
        usage,
    }
}
