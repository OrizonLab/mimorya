//! What a knowledge share may say (ADR-143): something shared, between two
//! of the company's units, the one shared not containing the other (its
//! members read it already), until a date to come; and its author not among
//! its readers, as no administrator opens business data to themself
//! (ADR-019, ADR-022): another administrator makes it.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::KnowledgeShare;
use crate::authz::OrgTree;

/// Why a share may not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareProblem {
    /// What it says is not allowed, in French.
    Invalid(&'static str),
    /// A unit the company does not have.
    UnknownUnit(Uuid),
    /// Its author would be among its readers.
    AuthorReads,
}

/// Why `share`, made at `now` by a member of `author_units`, may not be
/// made in the company of `tree`; `None` when it may.
pub fn share_problem(
    share: &KnowledgeShare,
    tree: &OrgTree,
    (author_units, now): (&[Uuid], DateTime<Utc>),
) -> Option<ShareProblem> {
    let (source, target) = (share.source_unit_id, share.target_unit_id);
    if !share.memory && !share.documents {
        return Some(ShareProblem::Invalid(
            "Partagez la mémoire, les documents, ou les deux",
        ));
    }
    if source == target {
        return Some(ShareProblem::Invalid(
            "Une unité partage ses connaissances avec une autre",
        ));
    }
    if share.expires_at.is_some_and(|at| at <= now) {
        return Some(ShareProblem::Invalid(
            "La date d’échéance doit être dans le futur",
        ));
    }
    if let Some(unknown) = [source, target]
        .into_iter()
        .find(|u| tree.get(*u).is_none())
    {
        return Some(ShareProblem::UnknownUnit(unknown));
    }
    if tree.ancestors(target).iter().any(|a| a.id == source) {
        return Some(ShareProblem::Invalid(
            "Les membres de cette unité lisent déjà les connaissances de l’unité qui la contient",
        ));
    }
    share
        .reaches(tree, author_units)
        .then_some(ShareProblem::AuthorReads)
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
