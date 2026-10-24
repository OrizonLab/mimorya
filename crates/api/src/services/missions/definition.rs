//! What a person entrusts as a mission (ADR-089): a title on one line, its
//! objective, and the success criteria its final answer is reviewed
//! against, hidden characters taken out: what they read is what the model
//! reads. And a mission's own limits: so many stages, so much cost, beyond
//! which it ends unfinished.

use mimorya_core::DomainError;
use mimorya_core::plain_text::{canonical, without_hidden};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_OBJECTIVE_CHARS: usize = 4000;
pub const MAX_CRITERIA: usize = 8;
pub const MAX_CRITERION_CHARS: usize = 300;

const BAD_TITLE: &str = "L’intitulé compte de 1 à 120 caractères.";
const BAD_OBJECTIVE: &str = "L’objectif compte de 1 à 4 000 caractères.";
const BAD_CRITERIA: &str = "Indiquez de 1 à 8 critères de réussite, de 300 caractères au plus \
     chacun : la réponse finale est vérifiée au regard de chacun.";

/// A mission's own limits, kept as when it was entrusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionLimits {
    pub max_stages: u32,
    pub max_cost_micros: i64,
}

/// Six stages, 12 USD: four times a stage's cost, with stages to spare for
/// those that cost less.
pub const MISSION_LIMITS: MissionLimits = MissionLimits {
    max_stages: 6,
    max_cost_micros: 12_000_000,
};

/// A mission as checked: its title, objective and criteria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedMission {
    pub title: String,
    pub objective: String,
    pub criteria: Vec<String>,
}

impl CheckedMission {
    pub fn check(title: &str, objective: &str, criteria: &[String]) -> Result<Self, DomainError> {
        let title = one_line(title);
        if !(1..=MAX_TITLE_CHARS).contains(&title.chars().count()) {
            return Err(DomainError::validation(BAD_TITLE));
        }
        let objective = canonical(without_hidden(objective).trim());
        if !(1..=MAX_OBJECTIVE_CHARS).contains(&objective.chars().count()) {
            return Err(DomainError::validation(BAD_OBJECTIVE));
        }
        let criteria: Vec<String> = criteria
            .iter()
            .map(|c| one_line(c))
            .filter(|c| !c.is_empty())
            .collect();
        let bounded = criteria
            .iter()
            .all(|c| c.chars().count() <= MAX_CRITERION_CHARS);
        if !(1..=MAX_CRITERIA).contains(&criteria.len()) || !bounded {
            return Err(DomainError::validation(BAD_CRITERIA));
        }
        Ok(Self {
            title,
            objective,
            criteria,
        })
    }
}

/// The text on one line, its blanks as single spaces, without hidden
/// characters.
fn one_line(text: &str) -> String {
    canonical(&without_hidden(text))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn criteria(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|t| (*t).to_string()).collect()
    }

    #[test]
    fn keeps_what_is_seen_and_bounds_it() {
        let mission = CheckedMission::check(
            "  Suivi\n des\u{200B} chantiers ",
            " Relis les comptes rendus.\nPuis propose un planning. ",
            &criteria(&[" Chaque intervention\nnommée ", "  ", "Un planning daté"]),
        )
        .unwrap();
        assert_eq!(mission.title, "Suivi des chantiers");
        assert_eq!(
            mission.objective,
            "Relis les comptes rendus.\nPuis propose un planning."
        );
        assert_eq!(
            mission.criteria,
            ["Chaque intervention nommée", "Un planning daté"]
        );
    }

    #[test]
    fn refuses_what_is_out_of_bounds() {
        let one = criteria(&["c"]);
        let check = |title: &str, objective: &str, criteria: &[String]| {
            CheckedMission::check(title, objective, criteria).map_err(|e| e.message)
        };
        assert_eq!(check(" \u{200B} ", "o", &one), Err(BAD_TITLE.into()));
        assert_eq!(check(&"t".repeat(121), "o", &one), Err(BAD_TITLE.into()));
        assert_eq!(check("t", "  ", &one), Err(BAD_OBJECTIVE.into()));
        let long = "o".repeat(MAX_OBJECTIVE_CHARS + 1);
        assert_eq!(check("t", &long, &one), Err(BAD_OBJECTIVE.into()));
        assert_eq!(check("t", "o", &criteria(&[" "])), Err(BAD_CRITERIA.into()));
        let nine = vec!["c".to_string(); MAX_CRITERIA + 1];
        assert_eq!(check("t", "o", &nine), Err(BAD_CRITERIA.into()));
        let wordy = vec!["c".repeat(MAX_CRITERION_CHARS + 1)];
        assert_eq!(check("t", "o", &wordy), Err(BAD_CRITERIA.into()));
        assert!(check(&"t".repeat(120), "o", &vec!["c".to_string(); 8]).is_ok());
    }
}
