//! The durations the register states: what the purge applies now (ADR-195),
//! and a shortening that waits with the day it applies.

use super::facts::RetentionFact;
use crate::retention::RetentionKind;
use crate::time_zone::PLATFORM_TIME_ZONE;

pub(super) fn conversations(facts: &[RetentionFact]) -> String {
    format!(
        "Conversations sans activité depuis {} : supprimées entières (messages, runs, appels \
         d’outils, validations) ; une conversation encore active est gardée entière",
        duration(facts, RetentionKind::Conversations)
    )
}

pub(super) fn episodes(facts: &[RetentionFact]) -> String {
    format!(
        "Épisodes : supprimés {} après leur enregistrement",
        duration(facts, RetentionKind::Episodes)
    )
}

pub(super) fn departed(facts: &[RetentionFact]) -> String {
    format!(
        "Mémoires personnelles d’un compte désactivé : supprimées {} après sa désactivation",
        duration(facts, RetentionKind::Departed)
    )
}

/// The duration of `kind` in effect, the platform's when the API gave
/// none; a shortening that waits said after it.
fn duration(facts: &[RetentionFact], kind: RetentionKind) -> String {
    let Some(fact) = facts.iter().find(|fact| fact.kind == kind) else {
        return amount(kind, kind.platform());
    };
    let now = amount(kind, fact.in_effect);
    match fact.shortening_at {
        Some(at) if fact.chosen != fact.in_effect => format!(
            "{now} ({} à partir du {})",
            amount(kind, fact.chosen),
            at.with_timezone(&PLATFORM_TIME_ZONE).format("%d/%m/%Y")
        ),
        _ => now,
    }
}

fn amount(kind: RetentionKind, value: i32) -> String {
    match kind.unit() {
        crate::retention::RetentionUnit::Months => format!("{value} mois"),
        crate::retention::RetentionUnit::Days if value == 1 => "1 jour".into(),
        crate::retention::RetentionUnit::Days => format!("{value} jours"),
    }
}
