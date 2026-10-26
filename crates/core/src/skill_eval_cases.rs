//! Evaluation cases of a skill (ADR-103): a request as a person would make
//! it, and the criteria a good answer meets, on which a version is tried
//! before it is made active. The request reaches the model as a user's turn
//! and the criteria the model that judges the answer: text that shows all
//! it holds, a criterion on one line each. Pure functions; the API applies
//! them.

use crate::plain_text::{canonical, shows_all_it_holds};
use crate::skill_rules::line_problem;

/// Cases of one skill: each is tried, on every evaluation, for the version
/// evaluated and for the active one.
pub const MAX_CASES_PER_SKILL: i64 = 10;
/// Criteria of one case.
pub const MAX_CRITERIA: usize = 8;
/// Characters of one criterion.
pub const MAX_CRITERION_CHARS: usize = 300;
/// Characters of a case's name, at least and at most.
pub const MIN_NAME_CHARS: u64 = 2;
pub const MAX_NAME_CHARS: u64 = 80;
/// Characters of a case's request.
pub const MAX_REQUEST_CHARS: u64 = 4000;

/// A case as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalCase {
    pub name: String,
    pub request: String,
    pub criteria: Vec<String>,
}

/// The case as stored, in canonical form (NFC: two names that look the
/// same are the same): its name and criteria trimmed, its blank criteria
/// left out, its request's Windows line endings made plain and its blanks
/// around removed.
pub fn normalized_case(name: &str, request: &str, criteria: &[String]) -> EvalCase {
    EvalCase {
        name: canonical(name.trim()),
        request: canonical(request.replace("\r\n", "\n").trim()),
        criteria: criteria
            .iter()
            .map(|c| canonical(c.trim()))
            .filter(|c| !c.is_empty())
            .collect(),
    }
}

/// Why a (normalized) case is refused, in French; `None` when it is valid.
pub fn case_problem(case: &EvalCase) -> Option<String> {
    if let Some(problem) = line_problem("Nom du cas", &case.name) {
        return Some(problem);
    }
    if !(MIN_NAME_CHARS..=MAX_NAME_CHARS).contains(&chars(&case.name)) {
        return Some(format!(
            "Nom du cas : de {MIN_NAME_CHARS} à {MAX_NAME_CHARS} caractères"
        ));
    }
    if case.request.is_empty() {
        return Some("La demande du cas est vide".into());
    }
    if chars(&case.request) > MAX_REQUEST_CHARS {
        return Some(format!(
            "La demande du cas compte {MAX_REQUEST_CHARS} caractères au plus"
        ));
    }
    if !shows_all_it_holds(&case.request) {
        return Some(
            "La demande ne contient que du texte visible, des retours à la ligne et des \
             tabulations"
                .into(),
        );
    }
    if case.criteria.is_empty() || case.criteria.len() > MAX_CRITERIA {
        return Some(format!(
            "De 1 à {MAX_CRITERIA} critères de réussite, un par ligne"
        ));
    }
    case.criteria
        .iter()
        .find_map(|criterion| criterion_problem("Critère", criterion))
}

/// Why one criterion, a case's or a version's (ADR-106), is refused, in
/// French, after its `label`: one visible line, not too long.
pub fn criterion_problem(label: &str, criterion: &str) -> Option<String> {
    line_problem(label, criterion).or_else(|| {
        (criterion.chars().count() > MAX_CRITERION_CHARS)
            .then(|| format!("{label} : {MAX_CRITERION_CHARS} caractères au plus"))
    })
}

fn chars(text: &str) -> u64 {
    u64::try_from(text.chars().count()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn criteria(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| (*l).to_owned()).collect()
    }

    fn case(name: &str, request: &str, lines: &[&str]) -> EvalCase {
        normalized_case(name, request, &criteria(lines))
    }

    #[test]
    fn a_case_is_stored_trimmed_without_blank_criteria() {
        let stored = case(
            "  Retards de la semaine ",
            "\r\nQuels retards cette semaine ?\r\nPar technicien.\r\n ",
            &[" Cite chaque intervention en retard. ", "", "  "],
        );
        assert_eq!(stored.name, "Retards de la semaine");
        assert_eq!(
            stored.request,
            "Quels retards cette semaine ?\nPar technicien."
        );
        assert_eq!(stored.criteria, ["Cite chaque intervention en retard."]);
        assert_eq!(case_problem(&stored), None);
    }

    #[test]
    fn a_case_has_a_request_and_one_to_eight_criteria() {
        assert!(case_problem(&case("Vide", " \n ", &["Un critère."])).is_some());
        assert!(case_problem(&case("Sans critère", "Demande", &[" "])).is_some());
        let nine = ["Critère."; 9];
        let problem = case_problem(&case("Trop", "Demande", &nine)).unwrap();
        assert!(problem.contains("1 à 8"), "{problem}");
        assert_eq!(case_problem(&case("Huit", "Demande", &nine[..8])), None);
    }

    #[test]
    fn a_criterion_is_one_visible_line_and_not_too_long() {
        let long = "é".repeat(MAX_CRITERION_CHARS + 1);
        let longest = "é".repeat(MAX_CRITERION_CHARS);
        assert!(case_problem(&case("Long", "Demande", &[&long])).is_some());
        assert_eq!(case_problem(&case("Juste", "Demande", &[&longest])), None);
        for hidden in ["Deux\nlignes", "caché\u{200B}", "a\u{202E}b"] {
            let problem = case_problem(&case("Caché", "Demande", &[hidden])).unwrap();
            assert!(problem.starts_with("Critère"), "{problem}");
        }
    }

    #[test]
    fn a_case_is_stored_in_canonical_form_within_its_lengths() {
        let composed = case("Caf\u{e9}", "Caf\u{e9} ?", &["Caf\u{e9}."]);
        let decomposed = case("Cafe\u{301}", "Cafe\u{301} ?", &["Cafe\u{301}."]);
        assert_eq!(composed, decomposed);
        assert!(case_problem(&case("C", "Demande", &["Critère."])).is_some());
        let name = "é".repeat(80);
        assert_eq!(case_problem(&case(&name, "Demande", &["Critère."])), None);
        let name = "é".repeat(81);
        assert!(case_problem(&case(&name, "Demande", &["Critère."])).is_some());
        // Counted once in canonical form: a character NFC writes in two.
        let half = usize::try_from(MAX_REQUEST_CHARS / 2).unwrap();
        let request = "\u{958}".repeat(half + 1);
        assert!(chars(&request) <= MAX_REQUEST_CHARS);
        let problem = case_problem(&case("Long", &request, &["Critère."])).unwrap();
        assert!(problem.contains("4000"), "{problem}");
    }

    #[test]
    fn the_name_is_a_visible_line_and_the_request_shows_all_it_holds() {
        let problem = case_problem(&case("Nom\u{200B}", "Demande", &["Critère."])).unwrap();
        assert!(problem.starts_with("Nom du cas"), "{problem}");
        for hidden in ["a\u{200B}b", "a\u{E0041}b", "a\u{7}b"] {
            assert!(
                case_problem(&case("Caché", hidden, &["Critère."])).is_some(),
                "{hidden:?}"
            );
        }
        assert_eq!(
            case_problem(&case("Lignes", "1.\tÉtape\n2.\tSuite", &["Critère."])),
            None
        );
    }
}
