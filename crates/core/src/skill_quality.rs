//! Quality criteria of a skill's version (ADR-106): what any answer that
//! follows the version must meet, written by the skill's administrators with
//! it. A run that loads the version has its answer reviewed on them by a
//! model, as an evaluation case's criteria are (ADR-103): text that shows all
//! it holds, a criterion on one line each. Pure functions; the API applies
//! them.

use crate::plain_text::canonical;
use crate::skill_eval_cases::{MAX_CRITERIA, criterion_problem};

/// Criteria of one version, at most.
pub const MAX_QUALITY_CRITERIA: usize = MAX_CRITERIA;

/// The criteria as stored, in canonical form (NFC): each trimmed, the
/// blank ones left out.
pub fn normalized_criteria(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| canonical(line.trim()))
        .filter(|line| !line.is_empty())
        .collect()
}

/// Why (normalized) criteria are refused, in French; `None` when they are
/// valid. A version may have none.
pub fn criteria_problem(criteria: &[String]) -> Option<String> {
    if criteria.len() > MAX_QUALITY_CRITERIA {
        return Some(format!(
            "{MAX_QUALITY_CRITERIA} critères de qualité au plus, un par ligne"
        ));
    }
    criteria
        .iter()
        .find_map(|criterion| criterion_problem("Critère de qualité", criterion))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_eval_cases::MAX_CRITERION_CHARS;

    fn lines(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|t| (*t).to_owned()).collect()
    }

    #[test]
    fn criteria_are_stored_trimmed_without_blank_ones() {
        let stored = normalized_criteria(&lines(&[" Cite chaque retard. ", "", "  ", "Date."]));
        assert_eq!(stored, ["Cite chaque retard.", "Date."]);
        assert_eq!(criteria_problem(&stored), None);
        assert_eq!(criteria_problem(&[]), None);
    }

    #[test]
    fn at_most_eight_criteria_each_one_visible_line_and_not_too_long() {
        let nine = lines(&["Critère."; 9]);
        let problem = criteria_problem(&nine).unwrap();
        assert!(problem.contains("8 critères"), "{problem}");
        assert_eq!(criteria_problem(&nine[..8]), None);
        let long = "é".repeat(MAX_CRITERION_CHARS + 1);
        assert!(
            criteria_problem(&[long])
                .unwrap()
                .contains("caractères au plus")
        );
        assert_eq!(criteria_problem(&["é".repeat(MAX_CRITERION_CHARS)]), None);
        for hidden in ["Deux\u{200B}mots", "Une\nautre ligne", "Tab\tulation"] {
            let problem = criteria_problem(&[hidden.to_owned()]);
            assert!(problem.is_some(), "{hidden:?}");
        }
    }
}
