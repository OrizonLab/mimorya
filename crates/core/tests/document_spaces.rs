//! Who reads and writes which documents (ADR-095), on the demo company: a
//! technician reads his team's, the units above and the company's, up to
//! his clearance, and deposits nowhere; a team lead deposits in his team's
//! space only, never above his clearance; the direction everywhere; the
//! agent never.

use std::collections::HashSet;

use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::demo::{self, unit, user};
use mimorya_core::documents::{
    DocumentSpace, can_write_document, document_read_filter, writable_spaces,
};
use mimorya_core::memory::UnitFilter;
use uuid::Uuid;

#[test]
fn a_technician_reads_his_team_the_units_above_and_the_company() {
    let engine = demo::authorization_engine();
    let lucas = demo::subject(user::LUCAS);
    let filter = document_read_filter(&engine, &lucas, &Actor::user());
    assert!(filter.company);
    assert_eq!(
        filter.units,
        UnitFilter::Only(HashSet::from([
            unit::TEAM_NORTH,
            unit::TECHNICAL,
            unit::COMPANY
        ]))
    );
    assert_eq!(filter.clearance, Sensitivity::Internal);
    assert_eq!(writable_spaces(&engine, &lucas, &Actor::user()), []);
}

#[test]
fn a_team_lead_deposits_in_his_team_under_his_clearance() {
    let engine = demo::authorization_engine();
    let julien = demo::subject(user::JULIEN);
    let me = Actor::user();
    let north = DocumentSpace::Unit(unit::TEAM_NORTH);
    assert_eq!(writable_spaces(&engine, &julien, &me), [north]);
    assert!(can_write_document(
        &engine,
        &julien,
        north,
        Sensitivity::Confidential,
        &me
    ));
    assert!(!can_write_document(
        &engine,
        &julien,
        north,
        Sensitivity::Restricted,
        &me
    ));
    for elsewhere in [
        DocumentSpace::Company,
        DocumentSpace::Unit(unit::TEAM_SOUTH),
        DocumentSpace::Unit(unit::TECHNICAL),
        DocumentSpace::Unit(Uuid::new_v4()),
    ] {
        assert!(
            !can_write_document(&engine, &julien, elsewhere, Sensitivity::Public, &me),
            "{elsewhere:?}"
        );
    }
}

#[test]
fn the_direction_deposits_everywhere_the_agent_nowhere() {
    let engine = demo::authorization_engine();
    let claire = demo::subject(user::CLAIRE);
    let spaces = writable_spaces(&engine, &claire, &Actor::user());
    assert_eq!(spaces.first(), Some(&DocumentSpace::Company));
    assert_eq!(spaces.len(), 1 + engine.org_tree().all().len());
    let agent = Actor::agent(Some(Uuid::new_v4()));
    assert_eq!(writable_spaces(&engine, &claire, &agent), []);
    assert_eq!(
        writable_spaces(&engine, &demo::subject(user::JULIEN), &agent),
        []
    );
    // Reading stays the agent's, as the person's.
    let filter = document_read_filter(&engine, &claire, &agent);
    assert_eq!(filter.units, UnitFilter::All);
}
