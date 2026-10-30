use chrono::{Duration, Utc};
use uuid::Uuid;

use super::*;
use crate::authz::Sensitivity;
use crate::demo::{org_tree, unit};

/// The north team's knowledge shared with the south team.
fn north_to_south() -> KnowledgeShare {
    KnowledgeShare {
        source_unit_id: unit::TEAM_NORTH,
        target_unit_id: unit::TEAM_SOUTH,
        include_descendants: false,
        memory: true,
        documents: true,
        ceiling: Sensitivity::Internal,
        expires_at: None,
    }
}

/// The problem of `share` made by a member of the administrative service.
fn problem(share: &KnowledgeShare) -> Option<ShareProblem> {
    share_problem(share, &org_tree(), (&[unit::ADMIN], Utc::now()))
}

fn invalid(share: &KnowledgeShare) -> bool {
    matches!(problem(share), Some(ShareProblem::Invalid(_)))
}

#[test]
fn a_share_says_something_between_two_units_until_a_date_to_come() {
    assert_eq!(problem(&north_to_south()), None);
    let nothing = KnowledgeShare {
        memory: false,
        documents: false,
        ..north_to_south()
    };
    assert!(invalid(&nothing));
    let itself = KnowledgeShare {
        target_unit_id: unit::TEAM_NORTH,
        ..north_to_south()
    };
    assert!(invalid(&itself));
    let past = KnowledgeShare {
        expires_at: Some(Utc::now() - Duration::minutes(1)),
        ..north_to_south()
    };
    assert!(invalid(&past));
    let to_come = KnowledgeShare {
        expires_at: Some(Utc::now() + Duration::days(1)),
        ..north_to_south()
    };
    assert_eq!(problem(&to_come), None);
}

#[test]
fn a_share_is_between_units_of_the_company_one_not_containing_the_other() {
    let elsewhere = Uuid::new_v4();
    let unknown = KnowledgeShare {
        target_unit_id: elsewhere,
        ..north_to_south()
    };
    assert_eq!(
        problem(&unknown),
        Some(ShareProblem::UnknownUnit(elsewhere))
    );
    // Its members read their service's knowledge already.
    let below = KnowledgeShare {
        source_unit_id: unit::TECHNICAL,
        target_unit_id: unit::TEAM_NORTH,
        ..north_to_south()
    };
    assert!(invalid(&below));
    // The other way round, a team's knowledge read by its service.
    let above = KnowledgeShare {
        source_unit_id: unit::TEAM_NORTH,
        target_unit_id: unit::TECHNICAL,
        ..north_to_south()
    };
    assert_eq!(problem(&above), None);
}

#[test]
fn its_author_is_never_among_its_readers() {
    let to_own_unit = KnowledgeShare {
        target_unit_id: unit::ADMIN,
        ..north_to_south()
    };
    assert_eq!(problem(&to_own_unit), Some(ShareProblem::AuthorReads));
    // To the company: its members only, not him; with the units under it, him.
    let company = KnowledgeShare {
        target_unit_id: unit::COMPANY,
        ..north_to_south()
    };
    assert_eq!(problem(&company), None);
    let everyone = KnowledgeShare {
        include_descendants: true,
        ..company
    };
    assert_eq!(problem(&everyone), Some(ShareProblem::AuthorReads));
    // His own unit's knowledge given to another: not read by him.
    let from_own_unit = KnowledgeShare {
        source_unit_id: unit::ADMIN,
        ..north_to_south()
    };
    assert_eq!(problem(&from_own_unit), None);
}
