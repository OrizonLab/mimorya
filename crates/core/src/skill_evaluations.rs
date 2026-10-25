//! What an evaluation of a skill's version says (ADR-104): each case
//! played for the version evaluated and, when another one is active, for
//! the active one beside it; a case the active version passed and the
//! evaluated one did not is a regression. Making a version active against
//! its last evaluation's regressions takes a confirmation. Pure functions;
//! the API applies them.

/// Evaluations of one skill kept, the oldest deleted as a new one is asked
/// for: their answers are read once, not archived.
pub const EVALUATIONS_KEPT_PER_SKILL: i64 = 5;
/// Characters of an answer kept with its result.
pub const MAX_ANSWER_CHARS: usize = 8000;
/// Tools of one case's play kept with its result, and characters of each
/// one's summary.
pub const MAX_TOOLS_KEPT: usize = 30;
pub const MAX_TOOL_SUMMARY_CHARS: usize = 300;

/// How one case went for one version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseOutcome {
    /// The answer met every criterion, as the judge read it.
    Passed,
    /// It missed one at least.
    Failed,
    /// No answer to judge, or no verdict: the run failed, or the judge.
    Error,
}

impl CaseOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Error => "error",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Passed, Self::Failed, Self::Error]
            .into_iter()
            .find(|o| o.as_str() == value)
    }
}

/// One case's results: for the version evaluated, and for the active one
/// when it was played beside it; `None` while not played yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaseResults {
    pub evaluated: Option<CaseOutcome>,
    pub reference: Option<CaseOutcome>,
}

impl CaseResults {
    /// The active version passed it, the evaluated one did not.
    pub fn regressed(&self) -> bool {
        self.reference == Some(CaseOutcome::Passed)
            && self.evaluated.is_some_and(|o| o != CaseOutcome::Passed)
    }
}

/// What an evaluation's cases add up to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub passed: usize,
    pub reference_passed: usize,
    pub regressions: usize,
}

pub fn summarize(cases: &[CaseResults]) -> Summary {
    let passed = |o: Option<CaseOutcome>| o == Some(CaseOutcome::Passed);
    Summary {
        passed: cases.iter().filter(|c| passed(c.evaluated)).count(),
        reference_passed: cases.iter().filter(|c| passed(c.reference)).count(),
        regressions: cases.iter().filter(|c| c.regressed()).count(),
    }
}

/// The last completed evaluation of a version about to be made active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastEvaluation {
    /// The version it was compared with, active then.
    pub reference_version: Option<i32>,
    pub regressions: usize,
}

/// Why making `version` active needs a confirmation, in French; `None` when
/// it does not: its last evaluation found no regression against the
/// version active now (one compared with another version says nothing of
/// this one), or the administrator confirmed.
pub fn activation_problem(
    version: i32,
    last: Option<LastEvaluation>,
    active_version: Option<i32>,
    confirmed: bool,
) -> Option<String> {
    let last = last?;
    if confirmed || last.regressions == 0 || last.reference_version != active_version {
        return None;
    }
    let cases = match last.regressions {
        1 => "1 cas réussi par la version active ne l’est plus".to_string(),
        n => format!("{n} cas réussis par la version active ne le sont plus"),
    };
    Some(format!(
        "Dernière évaluation de la version {version} : {cases}. Confirmez l’activation pour la \
         faire malgré tout."
    ))
}

/// `text` cut past `max` characters, an ellipsis marking the cut.
pub fn bounded(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use CaseOutcome::{Error, Failed, Passed};

    fn case(evaluated: Option<CaseOutcome>, reference: Option<CaseOutcome>) -> CaseResults {
        CaseResults {
            evaluated,
            reference,
        }
    }

    #[test]
    fn a_regression_is_a_case_the_active_version_passed_and_the_evaluated_one_did_not() {
        assert!(case(Some(Failed), Some(Passed)).regressed());
        assert!(case(Some(Error), Some(Passed)).regressed());
        assert!(!case(Some(Passed), Some(Passed)).regressed());
        assert!(!case(Some(Failed), Some(Failed)).regressed());
        assert!(!case(Some(Passed), Some(Failed)).regressed());
        // Nothing to compare with, or not played yet.
        assert!(!case(Some(Failed), None).regressed());
        assert!(!case(None, Some(Passed)).regressed());
    }

    #[test]
    fn an_evaluation_adds_its_cases_up() {
        let cases = [
            case(Some(Passed), Some(Passed)),
            case(Some(Failed), Some(Passed)),
            case(Some(Passed), Some(Error)),
            case(Some(Error), None),
        ];
        assert_eq!(
            summarize(&cases),
            Summary {
                passed: 2,
                reference_passed: 2,
                regressions: 1,
            }
        );
        assert_eq!(summarize(&[]), Summary::default());
    }

    #[test]
    fn regressions_against_the_active_version_take_a_confirmation() {
        let regressed = |regressions| {
            Some(LastEvaluation {
                reference_version: Some(2),
                regressions,
            })
        };
        let problem = activation_problem(3, regressed(2), Some(2), false).unwrap();
        assert!(problem.contains("version 3 : 2 cas réussis"), "{problem}");
        let one = activation_problem(3, regressed(1), Some(2), false).unwrap();
        assert!(
            one.contains("1 cas réussi par la version active ne l’est plus"),
            "{one}"
        );
        assert_eq!(activation_problem(3, regressed(2), Some(2), true), None);
        assert_eq!(activation_problem(3, regressed(0), Some(2), false), None);
        // Compared with a version no longer active, or never evaluated.
        assert_eq!(activation_problem(3, regressed(2), Some(1), false), None);
        assert_eq!(activation_problem(3, regressed(2), None, false), None);
        assert_eq!(activation_problem(3, None, Some(2), false), None);
    }

    #[test]
    fn an_outcome_reads_back_as_stored_and_a_text_is_cut() {
        for outcome in [Passed, Failed, Error] {
            assert_eq!(CaseOutcome::parse(outcome.as_str()), Some(outcome));
        }
        assert_eq!(CaseOutcome::parse("skipped"), None);
        assert_eq!(bounded("court", 10), "court");
        assert_eq!(bounded("éèêë", 2), "éè…");
    }
}
