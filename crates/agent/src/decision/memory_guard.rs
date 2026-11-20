//! What the decision service is asked before a memory is saved without a
//! person's validation, or a lesson proposed (ADR-213, D3): whether its
//! text holds health, banking or sensitive personal data. Said so at the
//! company's threshold, or any question left unanswered, the text is held
//! as sensitive; what each usage then does is the prudent side (a memory
//! waits for its owner's validation, a lesson is not kept).

use super::answer::Decided;
use super::question::{DecisionQuestion, DecisionRequest, DecisionState};
use super::usage::DecisionUsage;

/// What the decision reads of a title and of a text, at most: a memory's
/// fit whole (120 and 2 000 characters).
const MAX_TITLE_CHARS: usize = 200;
const MAX_TEXT_CHARS: usize = 6_000;

const QUESTIONS: [(&str, &str); 3] = [
    (
        "sante",
        "Ce texte contient-il des données de santé d’une personne identifiable (maladie, \
         handicap, soins, traitement, arrêt de travail, grossesse) ?",
    ),
    (
        "bancaire",
        "Ce texte contient-il des données bancaires ou de paiement (IBAN, numéro de carte, \
         coordonnées bancaires, identifiants de paiement) ?",
    ),
    (
        "personnel_sensible",
        "Ce texte contient-il des données personnelles sensibles sur une personne \
         identifiable (origine, opinions politiques, convictions religieuses, appartenance \
         syndicale, orientation sexuelle, condamnations, numéro de sécurité sociale, pièce \
         d’identité) ?",
    ),
];

/// The questions on `title` and `text`, nothing else.
pub fn memory_guard_request(title: &str, text: &str) -> DecisionRequest {
    DecisionRequest {
        usage: DecisionUsage::MemoryGuard.as_str(),
        state: DecisionState::new()
            .field("titre", title, MAX_TITLE_CHARS)
            .field("texte", text, MAX_TEXT_CHARS),
        questions: QUESTIONS
            .iter()
            .map(|(name, asked)| DecisionQuestion::noul(name, asked))
            .collect(),
    }
}

/// Whether the text is held as sensitive: a yes at `threshold`, or a
/// question unanswered.
pub fn holds_sensitive_data(decided: &Decided, threshold: f64) -> bool {
    QUESTIONS
        .iter()
        .any(|(name, _)| decided.yes(name).is_none_or(|yes| yes >= threshold))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::decision::{DecisionAnswer, EngineKind};

    fn decided(yes: &[(&str, f64)]) -> Decided {
        let answers = yes
            .iter()
            .map(|(name, yes)| ((*name).to_string(), DecisionAnswer::Noul { yes: *yes }))
            .collect::<BTreeMap<_, _>>();
        Decided {
            engine: EngineKind::Jev,
            model: "jev-1.14.0".into(),
            latency_ms: 1,
            answers,
        }
    }

    #[test]
    fn three_questions_on_the_title_and_the_text_alone() {
        let request = memory_guard_request("Préférence", "Envoyer le rapport le lundi");
        assert!(request.problem().is_none());
        assert_eq!(request.questions.len(), 3);
        let names: Vec<&str> = request
            .state
            .fields()
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(names, ["titre", "texte"]);
    }

    #[test]
    fn a_yes_at_the_threshold_or_an_answer_missing_holds_it_as_sensitive() {
        let clear = [
            ("sante", 0.1),
            ("bancaire", 0.05),
            ("personnel_sensible", 0.2),
        ];
        assert!(!holds_sensitive_data(&decided(&clear), 0.5));
        let health = [
            ("sante", 0.5),
            ("bancaire", 0.05),
            ("personnel_sensible", 0.2),
        ];
        assert!(holds_sensitive_data(&decided(&health), 0.5));
        let missing = [("sante", 0.1), ("bancaire", 0.05)];
        assert!(holds_sensitive_data(&decided(&missing), 0.5));
    }
}
