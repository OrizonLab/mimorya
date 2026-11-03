//! Allow policies widen the reach of whoever holds the permission in some
//! scope to resources outside it, where each resource is checked (ADR-168):
//! they never grant the permission itself, a deny still wins over them, and
//! the resource's sensitivity still holds against the person's clearance.

use mimorya_core::authz::{
    AbacPolicy, Actor, AuthorizationEngine, Condition, PolicyEffect, ResourceRef, Sensitivity,
};
use mimorya_core::demo::{self, tenant, unit, user};
use serde_json::json;
use uuid::Uuid;

fn policy(effect: PolicyEffect, condition: serde_json::Value) -> AbacPolicy {
    let condition: Condition = serde_json::from_value(condition).expect("condition");
    AbacPolicy {
        id: Uuid::new_v4(),
        description: "Politique de test".into(),
        effect,
        permissions: vec!["intervention:read".into()],
        condition,
    }
}

fn completed() -> serde_json::Value {
    json!({ "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "completed" } })
}

fn engine_with(policies: Vec<AbacPolicy>) -> AuthorizationEngine {
    let roles = demo::role_definitions()
        .iter()
        .map(demo::DemoRole::definition)
        .collect();
    let mut all = demo::policies();
    all.extend(policies);
    AuthorizationEngine::new(demo::org_tree(), roles, all)
}

/// Hugo's intervention, in the south team.
fn south_job(status: &str, sensitivity: Option<Sensitivity>) -> ResourceRef {
    ResourceRef::new("intervention", tenant::ID)
        .owner(Some(user::HUGO))
        .org_unit(Some(unit::TEAM_SOUTH))
        .sensitivity(sensitivity)
        .attribute("status", status)
}

fn reads(engine: &AuthorizationEngine, who: Uuid, job: &ResourceRef, actor: &Actor) -> bool {
    engine
        .check(&demo::subject(who), "intervention:read", job, actor)
        .allowed
}

#[test]
fn an_allow_policy_opens_resources_outside_the_scope_of_a_holder() {
    // Lucas, a technician, reads only his own interventions.
    let without = engine_with(vec![]);
    let job = south_job("completed", None);
    assert!(!reads(&without, user::LUCAS, &job, &Actor::user()));

    let engine = engine_with(vec![policy(PolicyEffect::Allow, completed())]);
    let decision = engine.check(
        &demo::subject(user::LUCAS),
        "intervention:read",
        &job,
        &Actor::user(),
    );
    assert!(decision.allowed);
    assert_eq!(decision.reason, "Politique de test");
    assert!(!reads(
        &engine,
        user::LUCAS,
        &south_job("planned", None),
        &Actor::user()
    ));
}

#[test]
fn an_allow_policy_never_grants_the_permission_itself() {
    let engine = engine_with(vec![policy(PolicyEffect::Allow, completed())]);
    let job = south_job("completed", None);
    // Paul administers the workspace without reading any intervention.
    assert!(!engine.has(
        &demo::subject(user::PAUL),
        "intervention:read",
        &Actor::user()
    ));
    assert!(!reads(&engine, user::PAUL, &job, &Actor::user()));
    // An agent restricted to other permissions holds none of it either.
    let restricted = Actor::agent(None).restricted_to(["planning:read"]);
    assert!(!reads(&engine, user::LUCAS, &job, &restricted));
    assert!(reads(&engine, user::LUCAS, &job, &Actor::agent(None)));
}

#[test]
fn a_deny_and_the_clearance_still_hold_against_an_allow_policy() {
    let on_agent = json!({ "op": "eq", "left": { "attr": "context.actorType" }, "right": { "value": "agent" } });
    let engine = engine_with(vec![
        policy(PolicyEffect::Allow, completed()),
        policy(PolicyEffect::Deny, on_agent),
    ]);
    let job = south_job("completed", None);
    assert!(reads(&engine, user::LUCAS, &job, &Actor::user()));
    assert!(!reads(&engine, user::LUCAS, &job, &Actor::agent(None)));
    // Lucas's clearance is internal.
    let confidential = south_job("completed", Some(Sensitivity::Confidential));
    assert!(!reads(&engine, user::LUCAS, &confidential, &Actor::user()));
}
