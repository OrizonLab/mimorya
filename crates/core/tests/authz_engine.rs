//! Authorization engine on the demo organisation, and agents acting for users.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use mimorya_core::authz::{
    Actor, AssignmentScope, DataScope, ResourceRef, RoleAssignment, Sensitivity, scope_includes,
};
use mimorya_core::demo::{self, role, tenant, unit, user};
use uuid::Uuid;

fn intervention(technician: Uuid, org_unit: Uuid, status: &str) -> ResourceRef {
    ResourceRef::new("intervention", tenant::ID)
        .owner(Some(technician))
        .org_unit(Some(org_unit))
        .attribute("status", status)
}

fn lucas_job() -> ResourceRef {
    intervention(user::LUCAS, unit::TEAM_NORTH, "in_progress")
}

fn hugo_job() -> ResourceRef {
    intervention(user::HUGO, unit::TEAM_SOUTH, "in_progress")
}

#[test]
fn a_technician_only_sees_their_own_interventions() {
    let engine = demo::authorization_engine();
    let lucas = demo::subject(user::LUCAS);
    let me = Actor::user();
    assert!(
        engine
            .check(&lucas, "intervention:read", &lucas_job(), &me)
            .allowed
    );
    assert!(
        !engine
            .check(&lucas, "intervention:read", &hugo_job(), &me)
            .allowed
    );
    let emma_job = intervention(user::EMMA, unit::TEAM_NORTH, "in_progress");
    assert!(
        !engine
            .check(&lucas, "intervention:read", &emma_job, &me)
            .allowed
    );
}

#[test]
fn a_team_lead_sees_their_team_not_the_other_team() {
    let engine = demo::authorization_engine();
    let julien = demo::subject(user::JULIEN);
    assert!(
        engine
            .check(&julien, "intervention:read", &lucas_job(), &Actor::user())
            .allowed
    );
    let decision = engine.check(&julien, "intervention:read", &hugo_job(), &Actor::user());
    assert!(!decision.allowed);
    assert!(decision.reason.contains("hors du périmètre"));
}

#[test]
fn the_technical_manager_sees_the_whole_department_subtree() {
    let engine = demo::authorization_engine();
    let marc = demo::subject(user::MARC);
    let scope = engine.data_scope(&marc, "intervention:read", &Actor::user());
    assert!(matches!(scope, DataScope::Restricted { .. }));
    assert!(scope_includes(
        &scope,
        Some(unit::TEAM_NORTH),
        Some(user::LUCAS)
    ));
    assert!(scope_includes(
        &scope,
        Some(unit::TEAM_SOUTH),
        Some(user::HUGO)
    ));
    assert!(
        engine
            .check(&marc, "intervention:read", &hugo_job(), &Actor::user())
            .allowed
    );
}

#[test]
fn the_director_sees_everything_in_the_tenant() {
    let engine = demo::authorization_engine();
    let claire = demo::subject(user::CLAIRE);
    assert_eq!(
        engine.data_scope(&claire, "intervention:read", &Actor::user()),
        DataScope::All
    );
}

#[test]
fn the_technical_administrator_manages_the_workspace_without_business_data() {
    let engine = demo::authorization_engine();
    let paul = demo::subject(user::PAUL);
    assert!(engine.has(&paul, "user:manage", &Actor::user()));
    assert!(
        !engine
            .check(&paul, "intervention:read", &lucas_job(), &Actor::user())
            .allowed
    );
}

#[test]
fn applies_the_abac_deny_policy_for_administrative_staff() {
    let engine = demo::authorization_engine();
    let isabelle = demo::subject(user::ISABELLE);
    let open = engine.check(&isabelle, "intervention:read", &lucas_job(), &Actor::user());
    assert!(!open.allowed);
    assert!(open.policy_id.is_some());
    let done = intervention(user::LUCAS, unit::TEAM_NORTH, "completed");
    assert!(
        engine
            .check(&isabelle, "intervention:read", &done, &Actor::user())
            .allowed
    );
}

#[test]
fn enforces_tenant_isolation_before_anything_else() {
    let engine = demo::authorization_engine();
    let claire = demo::subject(user::CLAIRE);
    let mut foreign = lucas_job();
    foreign.tenant_id = Uuid::new_v4();
    let decision = engine.check(&claire, "intervention:read", &foreign, &Actor::user());
    assert!(!decision.allowed);
    assert!(decision.reason.contains("autre entreprise"));
}

#[test]
fn enforces_sensitivity_clearance_except_for_the_owner() {
    let engine = demo::authorization_engine();
    let lucas = demo::subject(user::LUCAS); // clearance: internal
    let marc = demo::subject(user::MARC); // clearance: confidential
    let confidential = lucas_job().sensitivity(Some(Sensitivity::Confidential));
    assert!(
        engine
            .check(&lucas, "intervention:read", &confidential, &Actor::user())
            .allowed
    );
    assert!(
        engine
            .check(&marc, "intervention:read", &confidential, &Actor::user())
            .allowed
    );
    let restricted = hugo_job().sensitivity(Some(Sensitivity::Restricted));
    assert!(
        !engine
            .check(&marc, "intervention:read", &restricted, &Actor::user())
            .allowed
    );
}

#[test]
fn ignores_expired_assignments() {
    let now = Utc.with_ymd_and_hms(2026, 1, 10, 0, 0, 0).unwrap();
    let engine = demo::authorization_engine_at(Arc::new(move || now));
    let mut temp = demo::subject(user::LUCAS);
    temp.assignments = vec![RoleAssignment {
        role_id: role::TEAM_LEAD,
        scope: AssignmentScope::subtree(unit::TEAM_SOUTH),
        expires_at: Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
    }];
    assert!(
        !engine
            .check(&temp, "intervention:read", &hugo_job(), &Actor::user())
            .allowed
    );
}

#[test]
fn agents_never_exercise_non_delegable_permissions() {
    let engine = demo::authorization_engine();
    let paul = demo::subject(user::PAUL);
    let agent = Actor::agent(Some(Uuid::new_v4()));
    assert!(engine.has(&paul, "role:manage", &Actor::user()));
    assert!(!engine.has(&paul, "role:manage", &agent));
    assert!(!engine.has(&paul, "user:manage", &agent));
    assert!(!engine.has(&demo::subject(user::MARC), "approval:decide", &agent));
}

#[test]
fn agents_inherit_the_user_scope_and_can_be_narrowed_never_extended() {
    let engine = demo::authorization_engine();
    let julien = demo::subject(user::JULIEN);
    let agent = Actor::agent(Some(Uuid::new_v4()));
    assert!(
        engine
            .check(&julien, "intervention:read", &lucas_job(), &agent)
            .allowed
    );
    assert!(
        !engine
            .check(&julien, "intervention:read", &hugo_job(), &agent)
            .allowed
    );

    let narrowed = Actor::agent(Some(Uuid::new_v4())).restricted_to(["planning:read"]);
    assert!(
        !engine
            .check(&julien, "intervention:read", &lucas_job(), &narrowed)
            .allowed
    );

    let lucas = demo::subject(user::LUCAS);
    let widened = Actor::agent(Some(Uuid::new_v4())).restricted_to(["planning:apply"]);
    assert!(!engine.has(&lucas, "planning:apply", &widened));
}
