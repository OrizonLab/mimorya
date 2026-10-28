//! Authorization: scoped RBAC refined by ABAC policies, over an org tree of
//! unlimited depth.

pub mod conditions;
pub mod engine;
pub mod org_tree;
pub mod permissions;
pub mod sensitivity;
pub mod types;

pub use conditions::{ComparisonOp, Condition, Operand, evaluate_condition, reads_resource};
pub use engine::{AuthorizationEngine, Clock, scope_includes};
pub use org_tree::{OrgTree, OrgTreeError, OrgUnit, OrgUnitType};
pub use permissions::{
    Enforcement, NON_DELEGABLE_PERMISSIONS, PLATFORM_PERMISSIONS, PermissionInfo, is_non_delegable,
    is_permission, is_permission_pattern, matches_permission,
};
pub use sensitivity::Sensitivity;
pub use types::{
    AbacPolicy, Actor, ActorType, AssignmentScope, DataScope, Decision, HOOK_CHANNEL, PolicyEffect,
    ResourceRef, RoleAssignment, RoleDefinition, SLACK_CHANNEL, Subject, WEB_CHANNEL,
};
