//! Authorization engine: RBAC with scoped role assignments, refined by ABAC
//! policies. Pure and synchronous — callers load the inputs (roles, policies,
//! org tree, subject) and the engine decides. Every decision carries a reason
//! suitable for the audit log.
//!
//! Evaluation order for `check()`:
//!   1. tenant isolation (hard deny)
//!   2. agent ceiling: non-delegable permissions and per-run restrictions
//!   3. ABAC deny policies (deny overrides)
//!   4. sensitivity vs. subject clearance (owners always see their own data)
//!   5. RBAC grant within the assignment scope
//!   6. ABAC allow policies
//!   7. default deny
//!
//! `data_scope()` (and so `has()`, routes, tools and list scopes) applies the
//! agent ceiling and the deny policies whose condition reads no resource
//! attribute: they hold for the person and the actor whatever the resource.
//!
//! Hierarchical position never grants anything by itself: a manager sees their
//! team's data only through a role assignment scoped to that team.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::conditions::{evaluate_condition, reads_resource};
use super::org_tree::OrgTree;
use super::permissions::{is_non_delegable, matches_permission};
use super::types::{
    AbacPolicy, Actor, AssignmentScope, DataScope, Decision, PolicyEffect, ResourceRef,
    RoleDefinition, Subject,
};

pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

pub struct AuthorizationEngine {
    org_tree: OrgTree,
    roles: HashMap<Uuid, RoleDefinition>,
    policies: Vec<AbacPolicy>,
    /// Deny policies whose condition reads no resource attribute.
    holder_denials: Vec<usize>,
    now: Clock,
}

impl std::fmt::Debug for AuthorizationEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationEngine")
            .field("roles", &self.roles.len())
            .field("policies", &self.policies.len())
            .finish_non_exhaustive()
    }
}

impl AuthorizationEngine {
    pub fn new(org_tree: OrgTree, roles: Vec<RoleDefinition>, policies: Vec<AbacPolicy>) -> Self {
        let holder_denials = policies
            .iter()
            .enumerate()
            .filter(|(_, p)| p.effect == PolicyEffect::Deny && !reads_resource(&p.condition))
            .map(|(i, _)| i)
            .collect();
        Self {
            org_tree,
            roles: roles.into_iter().map(|r| (r.id, r)).collect(),
            policies,
            holder_denials,
            now: Arc::new(Utc::now),
        }
    }

    /// Replaces the clock used to expire role assignments (tests).
    pub fn with_clock(mut self, now: Clock) -> Self {
        self.now = now;
        self
    }

    pub fn org_tree(&self) -> &OrgTree {
        &self.org_tree
    }

    /// Scope in which `subject` holds `permission`. Used to pre-filter queries
    /// (SQL WHERE clauses, connector filters). ABAC refinements still apply per
    /// resource through `check()`.
    pub fn data_scope(&self, subject: &Subject, permission: &str, actor: &Actor) -> DataScope {
        if agent_ceiling_denies(permission, actor)
            || self.denied_to_holder(subject, permission, actor)
        {
            return DataScope::None;
        }

        let mut org_unit_ids = HashSet::new();
        let mut owner_user_ids = HashSet::new();
        let mut granted = false;
        let now = (self.now)();

        for assignment in &subject.assignments {
            if assignment.expires_at.is_some_and(|expires| expires <= now) {
                continue;
            }
            let Some(role) = self.roles.get(&assignment.role_id) else {
                continue;
            };
            if !role
                .permissions
                .iter()
                .any(|p| matches_permission(p, permission))
            {
                continue;
            }
            granted = true;
            match assignment.scope {
                AssignmentScope::Tenant => return DataScope::All,
                AssignmentScope::Own => {
                    owner_user_ids.insert(subject.user_id);
                }
                AssignmentScope::OrgUnit {
                    org_unit_id,
                    include_descendants: true,
                } => {
                    org_unit_ids.extend(self.org_tree.subtree(org_unit_id));
                }
                AssignmentScope::OrgUnit {
                    org_unit_id,
                    include_descendants: false,
                } => {
                    if self.org_tree.get(org_unit_id).is_some() {
                        org_unit_ids.insert(org_unit_id);
                    }
                }
            }
        }

        if !granted {
            return DataScope::None;
        }
        DataScope::Restricted {
            org_unit_ids,
            owner_user_ids,
        }
    }

    /// True if the subject holds the permission in at least some scope.
    pub fn has(&self, subject: &Subject, permission: &str, actor: &Actor) -> bool {
        self.data_scope(subject, permission, actor) != DataScope::None
    }

    pub fn check(
        &self,
        subject: &Subject,
        permission: &str,
        resource: &ResourceRef,
        actor: &Actor,
    ) -> Decision {
        self.check_with_context(subject, permission, resource, actor, Map::new())
    }

    pub fn check_with_context(
        &self,
        subject: &Subject,
        permission: &str,
        resource: &ResourceRef,
        actor: &Actor,
        context: Map<String, Value>,
    ) -> Decision {
        if resource.tenant_id != subject.tenant_id {
            return Decision::deny("Ressource d’une autre entreprise");
        }
        if agent_ceiling_denies(permission, actor) {
            return Decision::deny(format!("Permission {permission} non délégable à l’agent"));
        }

        let env = build_env(subject, resource, actor, context);
        let applicable: Vec<&AbacPolicy> = self
            .policies
            .iter()
            .filter(|p| {
                p.permissions
                    .iter()
                    .any(|pattern| matches_permission(pattern, permission))
            })
            .collect();

        let matching = |effect: PolicyEffect| {
            applicable
                .iter()
                .find(|p| p.effect == effect && evaluate_condition(&p.condition, &env))
                .map(|p| Decision {
                    allowed: effect == PolicyEffect::Allow,
                    reason: p.description.clone(),
                    policy_id: Some(p.id),
                })
        };

        if let Some(denied) = matching(PolicyEffect::Deny) {
            return denied;
        }

        let is_owner = resource
            .owner_user_id
            .is_some_and(|owner| owner == subject.user_id);
        if let Some(sensitivity) = resource.sensitivity.filter(|_| !is_owner) {
            let clearance = subject.clearance();
            if sensitivity > clearance {
                return Decision::deny(format!(
                    "Niveau de sensibilité {sensitivity} supérieur à l’habilitation {clearance}"
                ));
            }
        }

        let scope = self.data_scope(subject, permission, actor);
        if scope_includes(&scope, resource.org_unit_id, resource.owner_user_id) {
            return Decision::allow(format!("Rôle accordant {permission} sur ce périmètre"));
        }

        if let Some(allowed) = matching(PolicyEffect::Allow) {
            return allowed;
        }

        Decision::deny(if scope == DataScope::None {
            format!("Permission {permission} non accordée")
        } else {
            format!("Ressource hors du périmètre accordé pour {permission}")
        })
    }
}

impl AuthorizationEngine {
    /// Whether a deny policy about the person or the actor (no resource
    /// attribute in its condition) withdraws the permission.
    fn denied_to_holder(&self, subject: &Subject, permission: &str, actor: &Actor) -> bool {
        let applicable = self
            .holder_denials
            .iter()
            .map(|&i| &self.policies[i])
            .filter(|p| {
                p.permissions
                    .iter()
                    .any(|pattern| matches_permission(pattern, permission))
            });
        let mut env = None;
        for policy in applicable {
            let env = env.get_or_insert_with(|| holder_env(subject, actor));
            if evaluate_condition(&policy.condition, env) {
                return true;
            }
        }
        false
    }
}

fn agent_ceiling_denies(permission: &str, actor: &Actor) -> bool {
    if !actor.is_agent() {
        return false;
    }
    if is_non_delegable(permission) {
        return true;
    }
    actor
        .allowed_permissions
        .as_ref()
        .is_some_and(|allowed| !allowed.contains(permission))
}

/// The evaluation environment without any resource.
fn holder_env(subject: &Subject, actor: &Actor) -> Value {
    let mut env = build_env(
        subject,
        &ResourceRef::new("", subject.tenant_id),
        actor,
        Map::new(),
    );
    env["resource"] = json!({});
    env
}

fn build_env(
    subject: &Subject,
    resource: &ResourceRef,
    actor: &Actor,
    mut context: Map<String, Value>,
) -> Value {
    let mut resource_env = Map::new();
    resource_env.insert("type".into(), json!(resource.resource_type));
    if let Some(id) = &resource.id {
        resource_env.insert("id".into(), json!(id));
    }
    if let Some(unit) = resource.org_unit_id {
        resource_env.insert("orgUnitId".into(), json!(unit));
    }
    if let Some(owner) = resource.owner_user_id {
        resource_env.insert("ownerUserId".into(), json!(owner));
    }
    if let Some(sensitivity) = resource.sensitivity {
        resource_env.insert("sensitivityRank".into(), json!(sensitivity.rank()));
    }
    resource_env.insert(
        "attributes".into(),
        Value::Object(resource.attributes.clone()),
    );

    context.insert("actorType".into(), json!(actor.kind.as_str()));
    if let Some(channel) = &actor.channel {
        context.insert("channel".into(), json!(channel));
    }

    json!({
        "subject": {
            "userId": subject.user_id,
            "orgUnitIds": subject.org_unit_ids,
            "clearanceRank": subject.clearance().rank(),
            "attributes": subject.attributes,
        },
        "resource": resource_env,
        "context": context,
    })
}

/// Whether a resource falls inside a data scope (RBAC part only).
pub fn scope_includes(
    scope: &DataScope,
    org_unit_id: Option<Uuid>,
    owner_user_id: Option<Uuid>,
) -> bool {
    match scope {
        DataScope::All => true,
        DataScope::None => false,
        DataScope::Restricted {
            org_unit_ids,
            owner_user_ids,
        } => {
            org_unit_id.is_some_and(|unit| org_unit_ids.contains(&unit))
                || owner_user_id.is_some_and(|owner| owner_user_ids.contains(&owner))
        }
    }
}
