//! Memory visibility: personal memory stays personal, shared spaces follow
//! membership and `memory:*` scopes.

use std::collections::HashSet;

use mimorya_core::authz::Actor;
use mimorya_core::demo::{self, unit, user};
use mimorya_core::memory::{
    MemoryScope, MemoryTarget, UnitFilter, can_write_memory, memory_read_filter,
};

#[test]
fn a_technician_reads_personal_memory_plus_team_department_and_company_knowledge() {
    let engine = demo::authorization_engine();
    let filter = memory_read_filter(&engine, &demo::subject(user::LUCAS), &Actor::user());
    assert_eq!(filter.owner_user_id, user::LUCAS);
    assert!(filter.shared);
    assert_eq!(
        filter.org_units,
        UnitFilter::Only(HashSet::from([
            unit::TEAM_NORTH,
            unit::TECHNICAL,
            unit::COMPANY
        ]))
    );
}

#[test]
fn only_shared_spaces_within_the_write_scope_are_writable() {
    let engine = demo::authorization_engine();
    let me = Actor::user();
    let target = |scope, org_unit_id| MemoryTarget { scope, org_unit_id };

    let julien = demo::subject(user::JULIEN);
    assert!(can_write_memory(
        &engine,
        &julien,
        target(MemoryScope::User, None),
        &me
    ));
    assert!(can_write_memory(
        &engine,
        &julien,
        target(MemoryScope::OrgUnit, Some(unit::TEAM_NORTH)),
        &me
    ));
    assert!(!can_write_memory(
        &engine,
        &julien,
        target(MemoryScope::OrgUnit, Some(unit::TEAM_SOUTH)),
        &me
    ));
    assert!(!can_write_memory(
        &engine,
        &julien,
        target(MemoryScope::Tenant, None),
        &me
    ));

    let lucas = demo::subject(user::LUCAS);
    assert!(!can_write_memory(
        &engine,
        &lucas,
        target(MemoryScope::OrgUnit, Some(unit::TEAM_NORTH)),
        &me
    ));
    let claire = demo::subject(user::CLAIRE);
    assert!(can_write_memory(
        &engine,
        &claire,
        target(MemoryScope::Tenant, None),
        &me
    ));
}
