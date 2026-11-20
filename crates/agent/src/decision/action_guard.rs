//! What the decision service is asked of an action before it runs or is put
//! to a person's validation (ADR-213, D5): the tool, the action as it
//! describes it, its parameters and the served person's own e-mail domain,
//! nothing else; four yes-or-no questions (an external recipient, a large
//! amount, personal data, an action that cannot be undone) and a score of
//! its risk. Read back as the signals said at the usage's threshold, a
//! question left unanswered counting as one: lowering the threshold makes
//! the guard stricter, as for D3.

use std::borrow::Cow;

use mimorya_core::authz::Sensitivity;
use mimorya_core::documents::level_at;

use super::answer::{Decided, DecisionAnswer};
use super::question::{DecisionQuestion, DecisionRequest, DecisionState};
use super::usage::DecisionUsage;

const MAX_TOOL_CHARS: usize = 200;
const MAX_ACTION_CHARS: usize = 1_000;
/// What the decision reads of an action's parameters, at most.
pub const MAX_PARAMETERS_CHARS: usize = 4_000;
const MAX_DOMAIN_CHARS: usize = 255;

/// Each yes-or-no question, and the signal it raises, in French.
const QUESTIONS: [(&str, &str, &str); 4] = [
    (
        "destinataire_externe",
        "Cette action envoie-t-elle quelque chose, ou donne-t-elle accès à quelque chose, à \
         une personne ou une organisation hors de l’entreprise (une adresse d’un autre domaine \
         que le domaine interne donné) ?",
        "destinataire externe",
    ),
    (
        "montant_eleve",
        "Cette action engage-t-elle, paie-t-elle ou modifie-t-elle un montant important \
         (devis, facture, commande, paiement, remboursement) ?",
        "montant élevé",
    ),
    (
        "donnees_personnelles",
        "Cette action transmet-elle, publie-t-elle ou modifie-t-elle des données personnelles \
         d’une personne identifiable (coordonnées, santé, salaire, situation familiale…) ?",
        "données personnelles",
    ),
    (
        "irreversible",
        "Cette action est-elle difficile ou impossible à annuler (envoi, suppression, \
         paiement, publication) ?",
        "action irréversible",
    ),
];

const RISK: &str = "risque";
const RISK_LEVELS: [&str; 4] = [
    "Faible : sans conséquence notable si elle est faite par erreur",
    "Modéré : une erreur se corrige facilement, en interne",
    "Élevé : une erreur gênerait un client, un partenaire ou l’entreprise",
    "Critique : une erreur aurait des conséquences financières, juridiques ou humaines graves",
];
/// The level from which the risk is a signal: « élevé ».
const HIGH_RISK: Sensitivity = Sensitivity::Confidential;

/// An action about to run or to be put to a person's validation.
#[derive(Debug, Clone, Copy)]
pub struct GuardedAction<'a> {
    /// The tool's name and title.
    pub tool: &'a str,
    pub title: &'a str,
    /// The action as its tool describes it, if it does.
    pub action: Option<&'a str>,
    /// Its parameters, as JSON.
    pub parameters: &'a str,
    /// The served person's own e-mail domain, if known.
    pub internal_domain: Option<&'a str>,
}

/// The questions on `action`, nothing else.
pub fn action_guard_request(action: &GuardedAction<'_>) -> DecisionRequest {
    let tool = format!("{} ({})", action.title, action.tool);
    let mut state = DecisionState::new()
        .field("outil", &tool, MAX_TOOL_CHARS)
        .field("action", action.action.unwrap_or(""), MAX_ACTION_CHARS)
        .field("parametres", action.parameters, MAX_PARAMETERS_CHARS);
    if let Some(domain) = action.internal_domain {
        state = state.field("domaine_interne", domain, MAX_DOMAIN_CHARS);
    }
    let mut questions: Vec<DecisionQuestion> = QUESTIONS
        .iter()
        .map(|(name, asked, _)| DecisionQuestion::noul(name, asked))
        .collect();
    questions.push(DecisionQuestion::score(
        RISK,
        "Quel est le risque de cette action si elle était faite par erreur ?",
        &RISK_LEVELS,
    ));
    DecisionRequest {
        usage: DecisionUsage::ActionGuard.as_str(),
        state,
        questions,
    }
}

/// The signals the decision raises at `threshold`, in French: each yes at
/// the threshold, a risk at least « élevé » with that probability, and a
/// question it left unanswered.
pub fn guard_signals(decided: &Decided, threshold: f64) -> Vec<Cow<'static, str>> {
    let mut signals: Vec<Cow<'static, str>> = Vec::new();
    let mut unanswered = false;
    for (name, _, signal) in QUESTIONS {
        match decided.yes(name) {
            Some(yes) if yes >= threshold => signals.push(Cow::Borrowed(signal)),
            Some(_) => {}
            None => unanswered = true,
        }
    }
    match decided.answers.get(RISK) {
        Some(DecisionAnswer::Score {
            score,
            probabilities,
            ..
        }) => {
            if level_at(probabilities, *score, threshold) >= HIGH_RISK {
                signals.push(Cow::Borrowed("risque élevé"));
            }
        }
        _ => unanswered = true,
    }
    if unanswered {
        signals.push(Cow::Borrowed("décision incomplète"));
    }
    signals
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::decision::EngineKind;

    fn decided(yes: &[(&str, f64)], risk: Option<Vec<f64>>) -> Decided {
        let mut answers: BTreeMap<String, DecisionAnswer> = yes
            .iter()
            .map(|(name, yes)| ((*name).to_string(), DecisionAnswer::Noul { yes: *yes }))
            .collect();
        if let Some(probabilities) = risk {
            let answer = DecisionAnswer::Score {
                score: 1.0,
                confidence: 0.8,
                probabilities,
            };
            answers.insert(RISK.into(), answer);
        }
        Decided {
            engine: EngineKind::Jev,
            model: "jev-1.14.0".into(),
            latency_ms: 1,
            answers,
        }
    }

    const CLEAR: [(&str, f64); 4] = [
        ("destinataire_externe", 0.01),
        ("montant_eleve", 0.02),
        ("donnees_personnelles", 0.03),
        ("irreversible", 0.03),
    ];

    #[test]
    fn the_tool_its_action_its_parameters_and_the_internal_domain_alone() {
        let action = GuardedAction {
            tool: "mail_send",
            title: "Envoyer un e-mail",
            action: Some("Envoyer un e-mail à client@exemple.fr"),
            parameters: r#"{"to":["client@exemple.fr"]}"#,
            internal_domain: Some("neo-maintenance.example"),
        };
        let request = action_guard_request(&action);
        assert!(request.problem().is_none());
        assert_eq!(request.usage, "action_guard");
        let names: Vec<&str> = (request.state.fields().iter())
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, ["outil", "action", "parametres", "domaine_interne"]);
        assert_eq!(request.questions.len(), 5);
    }

    #[test]
    fn a_yes_at_the_threshold_a_high_risk_or_a_missing_answer_is_a_signal() {
        let low = Some(vec![0.6, 0.3, 0.06, 0.04]);
        assert!(guard_signals(&decided(&CLEAR, low.clone()), 0.5).is_empty());
        let mut external = CLEAR;
        external[0].1 = 0.5;
        assert_eq!(
            guard_signals(&decided(&external, low.clone()), 0.5),
            ["destinataire externe"]
        );
        // A high risk at the threshold, counted from the most critical.
        let high = Some(vec![0.1, 0.2, 0.4, 0.3]);
        assert_eq!(guard_signals(&decided(&CLEAR, high), 0.5), ["risque élevé"]);
        // Lowering the threshold makes the guard stricter.
        assert!(guard_signals(&decided(&CLEAR, low.clone()), 0.11).is_empty());
        assert_eq!(
            guard_signals(&decided(&CLEAR, low.clone()), 0.08),
            ["risque élevé"]
        );
        // A question or the risk unanswered.
        let missing = &CLEAR[..3];
        assert_eq!(
            guard_signals(&decided(missing, low), 0.5),
            ["décision incomplète"]
        );
        assert_eq!(
            guard_signals(&decided(&CLEAR, None), 0.5),
            ["décision incomplète"]
        );
    }
}
