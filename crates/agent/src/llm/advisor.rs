//! When a company may have Anthropic's advisor (ADR-207, step 3): the main
//! model consults the escalation model while it answers, a server tool of
//! Anthropic in beta. Offered only where the operator opens it (the
//! function is in beta, and an agreement not to retain data does not cover
//! every model), on the Anthropic provider at its official address, and for
//! a pair of models Anthropic accepts, as its documentation lists them. The
//! tool its runs are given: one consultation is an escalation, so a turn
//! may have as many as a run may have escalations; the advisor's prompt is
//! cached once three may be made, where caching starts to pay.

use super::pricing::model_alias;
use super::types::AdvisorTool;

/// The pairs Anthropic accepts, as its documentation lists them on
/// 2026-11-19
/// (<https://platform.claude.com/docs/en/agents-and-tools/tool-use/advisor-tool>):
/// each row, the main models and the advisors they accept.
const ADVISOR_PAIRS: &[(&[&str], &[&str])] = &[
    (
        &["claude-haiku-4-5", "claude-sonnet-4-6"],
        &[
            "claude-mythos-5-2",
            "claude-fable-5-2",
            "claude-mythos-5-1",
            "claude-fable-5-1",
            "claude-mythos-5",
            "claude-fable-5",
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-opus-4-8",
            "claude-opus-4-7",
            "claude-opus-4-6",
            "claude-sonnet-5-5",
            "claude-sonnet-5",
            "claude-haiku-5-5",
            "claude-sonnet-4-6",
        ],
    ),
    (
        &["claude-sonnet-5", "claude-haiku-5-5"],
        &[
            "claude-mythos-5-2",
            "claude-fable-5-2",
            "claude-mythos-5-1",
            "claude-fable-5-1",
            "claude-mythos-5",
            "claude-fable-5",
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-opus-4-8",
            "claude-opus-4-7",
            "claude-sonnet-5-5",
            "claude-sonnet-5",
            "claude-haiku-5-5",
        ],
    ),
    (
        &["claude-opus-4-6"],
        &[
            "claude-mythos-5-2",
            "claude-fable-5-2",
            "claude-mythos-5-1",
            "claude-fable-5-1",
            "claude-mythos-5",
            "claude-fable-5",
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-opus-4-8",
            "claude-opus-4-7",
            "claude-opus-4-6",
            "claude-sonnet-5-5",
            "claude-sonnet-5",
            "claude-haiku-5-5",
        ],
    ),
    (
        &["claude-opus-4-7", "claude-opus-4-8"],
        &[
            "claude-mythos-5-2",
            "claude-fable-5-2",
            "claude-mythos-5-1",
            "claude-fable-5-1",
            "claude-mythos-5",
            "claude-fable-5",
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-opus-4-8",
            "claude-opus-4-7",
            "claude-sonnet-5-5",
        ],
    ),
    (
        &[
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-fable-5",
            "claude-mythos-5",
        ],
        &[
            "claude-mythos-5-2",
            "claude-fable-5-2",
            "claude-mythos-5-1",
            "claude-fable-5-1",
            "claude-mythos-5",
            "claude-fable-5",
            "claude-opus-5-5",
            "claude-opus-5",
        ],
    ),
    (
        &["claude-fable-5-1", "claude-mythos-5-1"],
        &[
            "claude-mythos-5-2",
            "claude-fable-5-2",
            "claude-mythos-5-1",
            "claude-fable-5-1",
        ],
    ),
    (
        &["claude-fable-5-2", "claude-mythos-5-2"],
        &["claude-mythos-5-2", "claude-fable-5-2"],
    ),
    (
        &["claude-sonnet-5-5"],
        &[
            "claude-mythos-5-2",
            "claude-fable-5-2",
            "claude-mythos-5-1",
            "claude-fable-5-1",
            "claude-mythos-5",
            "claude-fable-5",
            "claude-opus-5-5",
            "claude-opus-5",
            "claude-sonnet-5-5",
        ],
    ),
];

/// From this many consultations a run may have, the advisor's prompt is
/// cached: below, the cache's writing costs more than its reads save.
const CACHED_FROM: u32 = 3;

/// The advisors Anthropic accepts for a main model (dated identifiers read
/// as their alias); none for a model it does not list.
pub fn advisors_for(main_model: &str) -> &'static [&'static str] {
    let main = model_alias(main_model);
    ADVISOR_PAIRS
        .iter()
        .find(|(mains, _)| mains.contains(&main))
        .map_or(&[], |(_, advisors)| advisors)
}

/// Whether Anthropic accepts `advisor` for `main_model`.
pub fn advisor_pair_accepted(main_model: &str, advisor: &str) -> bool {
    advisors_for(main_model).contains(&model_alias(advisor))
}

/// Where the platform stands for a company's advisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvisorOffer {
    /// The operator opened it (`MIMORYA_ANTHROPIC_ADVISOR`).
    pub operator_allows: bool,
    /// The company's runs go to Anthropic at its official address.
    pub anthropic_official: bool,
}

impl AdvisorOffer {
    /// Why the advisor may not be turned on whatever the models, in French;
    /// `None` when it may.
    pub fn problem(self) -> Option<String> {
        if !self.operator_allows {
            return Some(
                "L’exploitant de la plateforme n’a pas ouvert le conseiller d’Anthropic : la \
                 fonction est en bêta, et un accord de non-conservation des données ne couvre \
                 pas tous les modèles"
                    .into(),
            );
        }
        (!self.anthropic_official).then(|| {
            "Le conseiller n’est offert qu’avec le fournisseur Anthropic, à son adresse \
             officielle"
                .into()
        })
    }

    /// Why `advisor` may not advise `main_model` here, in French; `None`
    /// when it may.
    pub fn pair_problem(self, main_model: &str, advisor: &str) -> Option<String> {
        if let Some(problem) = self.problem() {
            return Some(problem);
        }
        if advisor_pair_accepted(main_model, advisor) {
            return None;
        }
        let accepted = advisors_for(main_model);
        Some(match accepted.is_empty() {
            true => format!("Anthropic n’offre pas de conseiller au modèle « {main_model} »"),
            false => format!(
                "« {advisor} » ne peut pas conseiller « {main_model} » : modèles possibles, {}",
                accepted.join(", ")
            ),
        })
    }
}

/// The tool a run is given: `advisor` consulted as many times in a turn as
/// a run may have escalations.
pub fn advisor_tool(advisor: &str, max_per_run: u32) -> AdvisorTool {
    AdvisorTool {
        model: advisor.to_string(),
        max_uses: max_per_run,
        cache: max_per_run >= CACHED_FROM,
    }
}

#[cfg(test)]
#[path = "advisor/tests.rs"]
mod tests;
