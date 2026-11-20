//! What the decision service is asked of a deposited document (ADR-213,
//! D4): its title and the text of its first pages, nothing else (not its
//! file name, its space nor who deposited it), asked its type (`choice`)
//! and its confidentiality (`score`, from public to restricted). Read back
//! as a level at the usage's threshold and a type with its confidence;
//! what becomes of them is the domain's (`documents::labels`).

use mimorya_core::documents::labels::LEVEL_DESCRIPTIONS;
use mimorya_core::documents::{Classified, DocumentType, level_at};

use super::answer::{Decided, DecisionAnswer};
use super::question::{DecisionQuestion, DecisionRequest, DecisionState};
use super::usage::DecisionUsage;

/// The pages read of a paged document; the others are read from their
/// start, up to `MAX_FIRST_TEXT_CHARS`.
pub const FIRST_PAGES: i32 = 3;
/// What the decision reads of the first pages, at most.
pub const MAX_FIRST_TEXT_CHARS: usize = 6_000;
const MAX_TITLE_CHARS: usize = 200;

const TYPE: &str = "type";
const LEVEL: &str = "confidentialite";

/// The questions on `title` and `first_text`, nothing else.
pub fn document_labels_request(title: &str, first_text: &str) -> DecisionRequest {
    let types: Vec<(&str, &str)> = DocumentType::ALL
        .iter()
        .map(|kind| (kind.as_str(), kind.description()))
        .collect();
    DecisionRequest {
        usage: DecisionUsage::DocumentLabels.as_str(),
        state: DecisionState::new()
            .field("titre", title, MAX_TITLE_CHARS)
            .field("debut", first_text, MAX_FIRST_TEXT_CHARS),
        questions: vec![
            DecisionQuestion::choice(
                TYPE,
                "Quel est le type de ce document, d’après son titre et le début de son texte ?",
                &types,
            ),
            DecisionQuestion::score(
                LEVEL,
                "Quel niveau de confidentialité ce document demande-t-il, d’après son titre et \
                 le début de son texte ? Le niveau le plus strict qu’il justifie.",
                &LEVEL_DESCRIPTIONS,
            ),
        ],
    }
}

/// The type and the level the decision gave, the level at `threshold`;
/// none when it gave no level.
pub fn classified(decided: &Decided, threshold: f64) -> Option<Classified> {
    let level = match decided.answers.get(LEVEL)? {
        DecisionAnswer::Score {
            score,
            probabilities,
            ..
        } => level_at(probabilities, *score, threshold),
        _ => return None,
    };
    let document_type = decided
        .choice(TYPE)
        .and_then(|(said, confidence)| Some((DocumentType::parse(said)?, confidence)));
    Some(Classified {
        document_type,
        level,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mimorya_core::authz::Sensitivity;

    use super::*;
    use crate::decision::EngineKind;

    fn decided(answers: Vec<(&str, DecisionAnswer)>) -> Decided {
        Decided {
            engine: EngineKind::Jev,
            model: "jev-1.14.0".into(),
            latency_ms: 1,
            answers: answers
                .into_iter()
                .map(|(name, answer)| (name.to_string(), answer))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn scored(probabilities: Vec<f64>) -> DecisionAnswer {
        DecisionAnswer::Score {
            score: 2.0,
            confidence: 0.8,
            probabilities,
        }
    }

    #[test]
    fn a_choice_and_a_score_on_the_title_and_the_first_pages_alone() {
        let request = document_labels_request("Contrat de maintenance", "Entre les soussignés…");
        assert!(request.problem().is_none());
        assert_eq!(request.usage, "document_labels");
        let names: Vec<&str> = (request.state.fields().iter())
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, ["titre", "debut"]);
        assert_eq!(request.questions.len(), 2);
        assert!(request.question(TYPE).is_some() && request.question(LEVEL).is_some());
        // The first pages beyond what is read are said truncated.
        let long = "a".repeat(MAX_FIRST_TEXT_CHARS + 10);
        let request = document_labels_request("Long", &long);
        assert!(request.state.chars() <= MAX_FIRST_TEXT_CHARS + MAX_TITLE_CHARS);
    }

    #[test]
    fn read_back_at_the_threshold_without_a_level_nothing() {
        let choice = DecisionAnswer::Choice {
            choice: "invoice".into(),
            confidence: 0.9,
            probabilities: BTreeMap::new(),
        };
        let both = decided(vec![
            (TYPE, choice.clone()),
            (LEVEL, scored(vec![0.1, 0.3, 0.4, 0.2])),
        ]);
        let read = classified(&both, 0.5).unwrap();
        assert_eq!(read.document_type, Some((DocumentType::Invoice, 0.9)));
        assert_eq!(read.level, Sensitivity::Confidential);
        assert_eq!(
            classified(&both, 0.2).unwrap().level,
            Sensitivity::Restricted
        );
        let unknown = DecisionAnswer::Choice {
            choice: "poem".into(),
            confidence: 0.9,
            probabilities: BTreeMap::new(),
        };
        let untyped = decided(vec![(TYPE, unknown), (LEVEL, scored(vec![1.0]))]);
        assert_eq!(classified(&untyped, 0.5).unwrap().document_type, None);
        assert_eq!(classified(&decided(vec![(TYPE, choice)]), 0.5), None);
    }
}
