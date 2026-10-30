//! Builds the authorization context of a request: the tenant's engine (cached
//! briefly per tenant) and the subject of the authenticated user, always
//! loaded from the database, never from the client.

use std::sync::Arc;
use std::time::Duration;

use mimorya_core::DomainError;
use mimorya_core::administration::is_tenant_wide;
use mimorya_core::authz::{
    Actor, AssignmentScope, AuthorizationEngine, DataScope, OrgTree, Subject,
};
use mimorya_db::Database;
use mimorya_db::repositories::org::{
    RoleRecord, TenantRecord, UserProfile, load_authorization_data, load_user_access,
};
use uuid::Uuid;

use super::invalidations::{Invalidation, Invalidations};
use super::tenant_cache::TenantCache;

/// The signed-in user, as resolved from the session cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthIdentity {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub session_id: Uuid,
    /// The account holds a temporary password it must replace first.
    pub must_change_password: bool,
}

pub struct AccessContext {
    pub tenant: TenantRecord,
    pub engine: Arc<AuthorizationEngine>,
    pub roles: Arc<Vec<RoleRecord>>,
    pub subject: Arc<Subject>,
    pub profile: UserProfile,
    /// Human readable grants, e.g. "Responsable d’équipe — Service technique et sous-unités".
    pub grants: Vec<String>,
}

impl AccessContext {
    pub fn org_tree(&self) -> &OrgTree {
        self.engine.org_tree()
    }

    /// Whether the user holds the permission in some scope (acting themselves).
    pub fn has(&self, permission: &str) -> bool {
        self.engine.has(&self.subject, permission, &Actor::user())
    }

    pub fn require(&self, permission: &str) -> Result<(), DomainError> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(DomainError::forbidden(format!(
                "Permission requise : {permission}"
            )))
        }
    }

    /// Whether the permission is effective for the user: a company-wide one
    /// (`TENANT_WIDE_PERMISSIONS`) only counts when held on the whole company.
    pub fn holds(&self, permission: &str) -> bool {
        if is_tenant_wide(permission) {
            self.require_tenant_wide(permission).is_ok()
        } else {
            self.has(permission)
        }
    }

    /// Company-wide screens and actions (`TENANT_WIDE_PERMISSIONS`): the
    /// permission must be held on the whole company, not on some unit.
    pub fn require_tenant_wide(&self, permission: &str) -> Result<(), DomainError> {
        match self
            .engine
            .data_scope(&self.subject, permission, &Actor::user())
        {
            DataScope::All => Ok(()),
            _ => Err(DomainError::forbidden(format!(
                "Permission requise sur toute l’entreprise : {permission}"
            ))),
        }
    }

    /// Tenant settings flag `demo`: the workspace holds fictional data.
    pub fn is_demo(&self) -> bool {
        self.tenant.settings.get("demo").and_then(|v| v.as_bool()) == Some(true)
    }
}

struct TenantAccess {
    tenant: TenantRecord,
    roles: Arc<Vec<RoleRecord>>,
    engine: Arc<AuthorizationEngine>,
}

pub struct AccessService {
    db: Database,
    cache: TenantCache<TenantAccess>,
    invalidations: Invalidations,
}

impl AccessService {
    pub fn new(db: Database, invalidations: Invalidations) -> Self {
        Self {
            db,
            cache: TenantCache::new(Duration::from_secs(30)),
            invalidations,
        }
    }

    /// After a change of the company's organization, roles or policies:
    /// loaded again, here and in the other processes.
    pub fn invalidate(&self, tenant_id: Uuid) {
        self.forget(tenant_id);
        self.invalidations.announce(Invalidation::Access(tenant_id));
    }

    /// Loaded again here: another process changed the company.
    pub(crate) fn forget(&self, tenant_id: Uuid) {
        self.cache.invalidate(tenant_id);
    }

    pub(crate) fn forget_all(&self) {
        self.cache.invalidate_all();
    }

    pub async fn context(&self, identity: AuthIdentity) -> anyhow::Result<AccessContext> {
        self.active_context(identity.tenant_id, identity.user_id)
            .await?
            .ok_or_else(|| DomainError::forbidden("Compte inactif").into())
    }

    /// The rights of an account as they stand now, outside any request (a
    /// run reads them before each step, ADR-035); `None` once the account is
    /// no longer active.
    pub async fn active_context(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Option<AccessContext>> {
        let tenant = self.tenant_access(tenant_id).await?;
        let mut tx = self.db.begin(tenant_id).await?;
        let access = load_user_access(&mut tx, user_id).await?;
        tx.commit().await?;
        let Some(access) = access.filter(|a| a.profile.status == "active") else {
            return Ok(None);
        };
        let tree = tenant.engine.org_tree();
        let grants = access
            .assignments
            .iter()
            .map(|a| {
                format!(
                    "{} — {}",
                    a.role_name,
                    describe_scope(&a.assignment.scope, tree)
                )
            })
            .collect();
        Ok(Some(AccessContext {
            tenant: tenant.tenant.clone(),
            engine: tenant.engine.clone(),
            roles: tenant.roles.clone(),
            subject: Arc::new(access.subject),
            profile: access.profile,
            grants,
        }))
    }

    async fn tenant_access(&self, tenant_id: Uuid) -> anyhow::Result<Arc<TenantAccess>> {
        // The generation is read before loading: an invalidation that happens
        // while the data loads then keeps the (possibly older) data out of the cache.
        let generation = match self.cache.get(tenant_id) {
            Ok(cached) => return Ok(cached),
            Err(generation) => generation,
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let data = load_authorization_data(&mut tx).await?;
        tx.commit().await?;
        let tree = OrgTree::new(data.units)?;
        let definitions = data.roles.iter().map(|r| r.definition.clone()).collect();
        let entry = Arc::new(TenantAccess {
            tenant: data.tenant,
            roles: Arc::new(data.roles),
            engine: Arc::new(
                AuthorizationEngine::new(tree, definitions, data.policies)
                    .with_knowledge_shares(data.knowledge_shares),
            ),
        });
        self.cache.store(tenant_id, generation, entry.clone());
        Ok(entry)
    }
}

pub fn describe_scope(scope: &AssignmentScope, tree: &OrgTree) -> String {
    match scope {
        AssignmentScope::Tenant => "toute l’entreprise".into(),
        AssignmentScope::Own => "ses propres données".into(),
        AssignmentScope::OrgUnit {
            org_unit_id,
            include_descendants,
        } => {
            let unit = tree
                .get(*org_unit_id)
                .map_or("unité inconnue", |u| u.name.as_str());
            if *include_descendants {
                format!("{unit} et sous-unités")
            } else {
                unit.to_string()
            }
        }
    }
}
