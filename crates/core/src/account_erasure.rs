//! When an account may be erased (ADR-189): a disabled account, whose
//! address the administrator typed, none of whose runs still lives, whose
//! decision no request awaits, who stands in for nobody's absence and leads
//! no unit. Each obstacle is said
//! in French, the first found; erasing then deletes the account and what
//! the platform keeps on the person, irreversibly.

/// What an erasure checks of the account, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErasureCandidate {
    /// `active`, `invited` or `disabled`.
    pub status: String,
    pub email: String,
    /// Runs on its behalf queued, running or awaiting a validation.
    pub live_runs: i64,
    /// Others' requests awaiting its decision as their manager.
    pub awaited_decisions: i64,
    /// Absences not yet ended in which it stands in for someone.
    pub stand_ins: i64,
    /// Units it leads.
    pub led_units: i64,
}

/// Why an account is not erased.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErasureObstacle {
    /// Disabling first takes its sessions, links and tokens (ADR-183).
    NotDisabled,
    /// The address typed is not the account's.
    NotConfirmed,
    /// A run of theirs would go while it works or waits.
    LiveRuns,
    /// Someone's request would lose who decides it.
    AwaitsDecisions,
    /// Someone's absence would lose its stand-in.
    StandsIn,
    /// A unit would lose its manager.
    Leads,
}

impl ErasureObstacle {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotDisabled => {
                "Désactivez d’abord le compte : seul un compte désactivé peut être effacé."
            }
            Self::NotConfirmed => {
                "L’adresse saisie n’est pas celle du compte : l’effacement n’a pas eu lieu."
            }
            Self::LiveRuns => {
                "Des demandes de ce compte sont en cours ou attendent une validation : l’effacement attend qu’elles se terminent."
            }
            Self::AwaitsDecisions => {
                "Des demandes attendent la décision de ce compte : l’effacement attend qu’elles soient décidées ou échues."
            }
            Self::StandsIn => {
                "Ce compte remplace un collègue absent : désignez d’abord un autre remplaçant."
            }
            Self::Leads => "Ce compte dirige une unité : désignez d’abord un autre responsable.",
        }
    }
}

/// Whether `typed` names the account at `email`: case and surrounding
/// spaces aside.
pub fn confirms(typed: &str, email: &str) -> bool {
    typed.trim().to_lowercase() == email.trim().to_lowercase()
}

/// The first reason `candidate` may not be erased with `typed` as its
/// confirmation; `None` when it may.
pub fn obstacle(candidate: &ErasureCandidate, typed: &str) -> Option<ErasureObstacle> {
    if candidate.status != "disabled" {
        Some(ErasureObstacle::NotDisabled)
    } else if !confirms(typed, &candidate.email) {
        Some(ErasureObstacle::NotConfirmed)
    } else if candidate.live_runs > 0 {
        Some(ErasureObstacle::LiveRuns)
    } else if candidate.awaited_decisions > 0 {
        Some(ErasureObstacle::AwaitsDecisions)
    } else if candidate.stand_ins > 0 {
        Some(ErasureObstacle::StandsIn)
    } else if candidate.led_units > 0 {
        Some(ErasureObstacle::Leads)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled() -> ErasureCandidate {
        ErasureCandidate {
            status: "disabled".into(),
            email: "lucas.petit@neo-maintenance.example".into(),
            live_runs: 0,
            awaited_decisions: 0,
            stand_ins: 0,
            led_units: 0,
        }
    }

    const TYPED: &str = " Lucas.Petit@Neo-Maintenance.example ";

    #[test]
    fn a_disabled_account_confirmed_with_no_tie_is_erased() {
        assert_eq!(obstacle(&disabled(), TYPED), None);
    }

    #[test]
    fn an_account_still_in_use_is_not() {
        for status in ["active", "invited"] {
            let candidate = ErasureCandidate {
                status: status.into(),
                ..disabled()
            };
            assert_eq!(
                obstacle(&candidate, TYPED),
                Some(ErasureObstacle::NotDisabled)
            );
        }
    }

    #[test]
    fn another_address_or_none_does_not_confirm() {
        for typed in [
            "",
            "lucas.petit@neo-maintenance",
            "paul.renaud@neo-maintenance.example",
        ] {
            assert_eq!(
                obstacle(&disabled(), typed),
                Some(ErasureObstacle::NotConfirmed),
                "{typed}"
            );
        }
    }

    #[test]
    fn what_ties_the_account_to_others_is_said_first_found() {
        let tied = ErasureCandidate {
            live_runs: 1,
            awaited_decisions: 1,
            stand_ins: 1,
            led_units: 1,
            ..disabled()
        };
        assert_eq!(obstacle(&tied, TYPED), Some(ErasureObstacle::LiveRuns));
        let tied = ErasureCandidate {
            live_runs: 0,
            ..tied
        };
        assert_eq!(
            obstacle(&tied, TYPED),
            Some(ErasureObstacle::AwaitsDecisions)
        );
        let tied = ErasureCandidate {
            awaited_decisions: 0,
            ..tied
        };
        assert_eq!(obstacle(&tied, TYPED), Some(ErasureObstacle::StandsIn));
        let tied = ErasureCandidate {
            stand_ins: 0,
            ..tied
        };
        assert_eq!(obstacle(&tied, TYPED), Some(ErasureObstacle::Leads));
    }
}
