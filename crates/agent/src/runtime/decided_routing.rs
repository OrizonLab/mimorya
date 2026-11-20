//! The router's classification asked of the decision service (ADR-213, D1):
//! a choice between the main model and the escalation model on the request
//! alone. Active, « escalade », or « principal » with a confidence below the
//! company's threshold, starts the run on the escalation model; observed,
//! the decision is journaled and changes nothing. Undecided — off, every
//! engine failing, an answer off its question — the main model's own
//! classification decides, as before. The escalation's bounds (its month's
//! budget, its escalations per run) are the only gate either way. Each call
//! that answered is counted in the run.

use super::state::RunCheckpoint;
use super::types::{RunConfig, RuntimeDeps};
use crate::decision::{
    Counted, DecisionMode, DecisionOutcome, DecisionQuestion, DecisionRequest, DecisionState,
    DecisionUsage,
};
use crate::runtime::ports::RunEvent;

/// What the decision reads of the request, at most: as the main model's
/// classification does.
const MAX_ROUTED_CHARS: usize = 2_000;
const QUESTION: &str = "niveau";
const MAIN: &str = "principal";
const ESCALATION: &str = "escalade";

/// What the decision service said of a run's level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Routed {
    Escalation,
    Main,
    /// Off, observed, or no engine decided: the classification decides.
    Undecided,
}

/// The decision of the run's level, when the company set its usage.
pub(super) async fn decided_level(
    deps: &RuntimeDeps,
    config: &RunConfig,
    state: &mut RunCheckpoint,
) -> anyhow::Result<Routed> {
    let Some(decisions) = &deps.decisions else {
        return Ok(Routed::Undecided);
    };
    let policy = decisions.policy(DecisionUsage::ModelRouting);
    if policy.mode == DecisionMode::Off {
        return Ok(Routed::Undecided);
    }
    let (outcome, counted) = decisions
        .service
        .decide_counted(&routing_request(&config.objective))
        .await;
    count(deps, state, &counted).await?;
    let escalate = match &outcome {
        DecisionOutcome::Decided(decided) => decided
            .choice(QUESTION)
            .map(|(choice, confidence)| hard(choice, confidence, policy.threshold)),
        DecisionOutcome::Failed => None,
    };
    if policy.mode == DecisionMode::Observe {
        tracing::info!(
            target: "mimorya::decision",
            usage = DecisionUsage::ModelRouting.as_str(),
            mode = "observe",
            would_escalate = ?escalate,
            "Decision observed: the run is routed as without it"
        );
        return Ok(Routed::Undecided);
    }
    Ok(match escalate {
        Some(true) => Routed::Escalation,
        Some(false) => Routed::Main,
        None => Routed::Undecided,
    })
}

/// Whether the run starts on the escalation model: named, or the main
/// model named with too little confidence.
fn hard(choice: &str, confidence: f64, threshold: f64) -> bool {
    choice == ESCALATION || confidence < threshold
}

/// The request alone, at most `MAX_ROUTED_CHARS` of it, and the choice.
fn routing_request(objective: &str) -> DecisionRequest {
    DecisionRequest {
        usage: DecisionUsage::ModelRouting.as_str(),
        state: DecisionState::new().field("demande", objective, MAX_ROUTED_CHARS),
        questions: vec![DecisionQuestion::choice(
            QUESTION,
            "Quel modèle doit traiter la demande qu’une personne adresse à l’assistant de \
             son entreprise ?",
            &[
                (
                    MAIN,
                    "La plupart des demandes : une question, une recherche, une action simple.",
                ),
                (
                    ESCALATION,
                    "Une demande difficile : raisonnement en plusieurs étapes, analyse de \
                     plusieurs sources, rédaction longue ou délicate, décision aux conséquences \
                     importantes.",
                ),
            ],
        )],
    }
}

/// The calls that answered, added to the run's totals as its other calls.
async fn count(
    deps: &RuntimeDeps,
    state: &mut RunCheckpoint,
    counted: &[Counted],
) -> anyhow::Result<()> {
    for call in counted {
        state.usage.input_tokens += call.input_tokens;
        state.usage.output_tokens += call.output_tokens;
        state
            .usage
            .add_cost(call.cost_micros.unwrap_or(0), call.charged_micros);
        deps.ports
            .events
            .emit(RunEvent::Usage {
                input_tokens: call.input_tokens,
                output_tokens: call.output_tokens,
                cost_micros: call.cost_micros.map(|_| call.charged_micros),
            })
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_escalation_model_named_or_too_little_confidence_escalates() {
        assert!(hard(ESCALATION, 0.99, 0.5));
        assert!(hard(MAIN, 0.4, 0.5));
        assert!(!hard(MAIN, 0.5, 0.5));
        assert!(!hard(MAIN, 0.9, 0.5));
    }

    #[test]
    fn the_request_alone_is_sent_cut_to_its_bound() {
        let request = routing_request(&"é".repeat(5_000));
        assert!(request.problem().is_none());
        let [(name, text)] = request.state.fields() else {
            panic!("the request alone")
        };
        assert_eq!(name, "demande");
        assert!(
            text.chars().count() <= MAX_ROUTED_CHARS,
            "{}",
            text.chars().count()
        );
    }
}
