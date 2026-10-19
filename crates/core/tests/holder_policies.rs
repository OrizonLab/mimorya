//! Deny policies whose condition is about the person or the actor apply
//! wherever the permission is checked (`data_scope`, `has`): routes, tools and
//! list scopes, not only per-resource checks. Those reading the resource only
//! apply where each resource is checked.

use mimorya_core::authz::{
    AbacPolicy, Actor, AuthorizationEngine, Condition, DataScope, PolicyEffect, ResourceRef,
};
use mimorya_core::demo::{self, tenant, user};
use serde_json::json;
use uuid::Uuid;

fn engine_with(policy: AbacPolicy) -> AuthorizationEngine {
    let roles = demo::role_definitions()
        .iter()
        .map(demo::DemoRole::definition)
        .collect();
    let mut policies = demo::policies();
    policies.push(policy);
    AuthorizationEngine::new(demo::org_tree(), roles, policies)
}

fn deny(permissions: &[&str], condition: serde_json::Value) -> AbacPolicy {
    let condition: Condition = serde_json::from_value(condition).expect("condition");
    AbacPolicy {
        id: Uuid::new_v4(),
        description: "Refus de test".into(),
        effect: PolicyEffect::Deny,
        permissions: permissions.iter().map(|p| (*p).to_string()).collect(),
        condition,
    }
}

#[test]
fn a_deny_about_the_actor_withdraws_the_permission_from_the_agent_only() {
    let engine = engine_with(deny(
        &["planning:apply"],
        json!({ "op": "eq", "left": { "attr": "context.actorType" }, "right": { "value": "agent" } }),
    ));
    let marc = demo::subject(user::MARC);
    let agent = Actor::agent(Some(Uuid::new_v4()));
    assert!(engine.has(&marc, "planning:apply", &Actor::user()));
    assert!(!engine.has(&marc, "planning:apply", &agent));
    assert_eq!(
        engine.data_scope(&marc, "planning:apply", &agent),
        DataScope::None
    );
    // Other permissions are untouched.
    assert!(engine.has(&marc, "planning:propose", &agent));
}

#[test]
fn a_deny_about_the_person_withdraws_the_permission_everywhere() {
    let engine = engine_with(deny(
        &["agent:use", "intervention:*"],
        json!({ "op": "eq", "left": { "attr": "subject.userId" }, "right": { "value": user::LUCAS } }),
    ));
    let lucas = demo::subject(user::LUCAS);
    let own_job = ResourceRef::new("intervention", tenant::ID)
        .owner(Some(user::LUCAS))
        .attribute("status", "in_progress");
    assert!(!engine.has(&lucas, "agent:use", &Actor::user()));
    assert!(
        !engine
            .check(&lucas, "intervention:read", &own_job, &Actor::user())
            .allowed
    );
    let marc = demo::subject(user::MARC);
    assert!(engine.has(&marc, "agent:use", &Actor::user()));
}

#[test]
fn a_deny_reading_the_resource_only_applies_per_resource() {
    // Even a negation, true when the resource is unknown, stays per resource.
    let engine = engine_with(deny(
        &["intervention:read"],
        json!({ "not": { "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "completed" } } }),
    ));
    let lucas = demo::subject(user::LUCAS);
    assert!(engine.has(&lucas, "intervention:read", &Actor::user()));
    let job = |status: &str| {
        ResourceRef::new("intervention", tenant::ID)
            .owner(Some(user::LUCAS))
            .attribute("status", status)
    };
    let me = Actor::user();
    assert!(
        engine
            .check(&lucas, "intervention:read", &job("completed"), &me)
            .allowed
    );
    assert!(
        !engine
            .check(&lucas, "intervention:read", &job("in_progress"), &me)
            .allowed
    );
}
