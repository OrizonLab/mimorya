use chrono::{Duration, Utc};
use serde_json::Map;
use uuid::Uuid;

use super::*;
use crate::demo::{org_tree, unit};

fn member_of(unit_id: Uuid, clearance: Sensitivity) -> Subject {
    Subject {
        tenant_id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        org_unit_ids: vec![unit_id],
        assignments: Vec::new(),
        clearance: Some(clearance),
        attributes: Map::new(),
    }
}

/// The Direction's memory shared with the technical department.
fn direction_to_technical() -> KnowledgeShare {
    KnowledgeShare {
        id: Uuid::new_v4(),
        source_unit_id: unit::DIRECTION,
        target_unit_id: unit::TECHNICAL,
        include_descendants: false,
        memory: true,
        documents: false,
        ceiling: Sensitivity::Internal,
        expires_at: None,
    }
}

#[test]
fn a_unit_s_members_read_what_is_shared_with_it_and_only_that() {
    let (tree, now) = (org_tree(), Utc::now());
    let shares = [direction_to_technical()];
    let technical = member_of(unit::TECHNICAL, Sensitivity::Confidential);
    assert_eq!(
        shared_units(&shares, &tree, &technical, (SharedKnowledge::Memory, now)),
        [(unit::DIRECTION, Sensitivity::Internal)]
    );
    // Its documents are not shared.
    let documents = (SharedKnowledge::Documents, now);
    assert!(shared_units(&shares, &tree, &technical, documents).is_empty());
    // Nor is it shared with the teams under it, unless said so.
    let north = member_of(unit::TEAM_NORTH, Sensitivity::Confidential);
    let memory = (SharedKnowledge::Memory, now);
    assert!(shared_units(&shares, &tree, &north, memory).is_empty());
    let below = [KnowledgeShare {
        include_descendants: true,
        ..direction_to_technical()
    }];
    assert_eq!(
        shared_units(&below, &tree, &north, memory),
        [(unit::DIRECTION, Sensitivity::Internal)]
    );
    // Nor with another unit.
    let south_only = [KnowledgeShare {
        target_unit_id: unit::TEAM_SOUTH,
        ..direction_to_technical()
    }];
    assert!(shared_units(&south_only, &tree, &north, memory).is_empty());
}

#[test]
fn what_is_read_stays_under_the_share_and_the_reader_s_clearance() {
    let (tree, now) = (org_tree(), Utc::now());
    let memory = (SharedKnowledge::Memory, now);
    let wide = [KnowledgeShare {
        ceiling: Sensitivity::Restricted,
        ..direction_to_technical()
    }];
    let cleared_low = member_of(unit::TECHNICAL, Sensitivity::Public);
    assert_eq!(
        shared_units(&wide, &tree, &cleared_low, memory),
        [(unit::DIRECTION, Sensitivity::Public)]
    );
    // Of two shares of a unit, the higher is read.
    let both = [
        direction_to_technical(),
        KnowledgeShare {
            id: Uuid::new_v4(),
            ceiling: Sensitivity::Confidential,
            ..direction_to_technical()
        },
    ];
    let cleared = member_of(unit::TECHNICAL, Sensitivity::Restricted);
    assert_eq!(
        shared_units(&both, &tree, &cleared, memory),
        [(unit::DIRECTION, Sensitivity::Confidential)]
    );
}

#[test]
fn a_share_past_its_date_opens_nothing() {
    let (tree, now) = (org_tree(), Utc::now());
    let technical = member_of(unit::TECHNICAL, Sensitivity::Internal);
    let ended = [KnowledgeShare {
        expires_at: Some(now - Duration::minutes(1)),
        ..direction_to_technical()
    }];
    let memory = (SharedKnowledge::Memory, now);
    assert!(shared_units(&ended, &tree, &technical, memory).is_empty());
    let running = [KnowledgeShare {
        expires_at: Some(now + Duration::days(1)),
        ..direction_to_technical()
    }];
    assert_eq!(shared_units(&running, &tree, &technical, memory).len(), 1);
}
