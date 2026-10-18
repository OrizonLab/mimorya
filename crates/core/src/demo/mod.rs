//! FICTIONAL demo company « Néo Maintenance »: organisation, roles, staff and
//! an example policy, used by the seed, the tests and the reference scenario.

mod ids;
mod org_units;
mod policies;
mod roles;
mod users;

use crate::authz::{AuthorizationEngine, Clock, OrgTree, Subject};

pub use ids::{demo_id, role, tenant, unit, user};
pub use org_units::org_units;
pub use policies::policies;
pub use roles::{DemoRole, role_definitions};
pub use users::{DemoUser, users};

pub fn org_tree() -> OrgTree {
    OrgTree::new(org_units()).expect("valid demo organisation")
}

pub fn authorization_engine() -> AuthorizationEngine {
    let roles = role_definitions()
        .iter()
        .map(DemoRole::definition)
        .collect();
    AuthorizationEngine::new(org_tree(), roles, policies())
}

pub fn authorization_engine_at(now: Clock) -> AuthorizationEngine {
    authorization_engine().with_clock(now)
}

/// Subject of a demo user. Panics for an unknown user (tests and seed only).
pub fn subject(user_id: uuid::Uuid) -> Subject {
    let user = users()
        .into_iter()
        .find(|u| u.id == user_id)
        .unwrap_or_else(|| panic!("Unknown demo user {user_id}"));
    Subject {
        tenant_id: tenant::ID,
        user_id: user.id,
        org_unit_ids: vec![user.primary_unit_id],
        assignments: user.assignments,
        clearance: Some(user.clearance),
        attributes: user.attributes,
    }
}
