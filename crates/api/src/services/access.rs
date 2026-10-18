//! Builds the authorization context of a request: the tenant's engine (cached
//! briefly per tenant) and the subject of the authenticated user, always
//! loaded from the database, never from the client.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// The signed-in user, as resolved from the session cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthIdentity {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub session_id: Uuid,
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
    loaded_at: Instant,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Uuid, Arc<TenantAccess>>,
    /// Bumped by every invalidation: data loaded across one is not cached,
    /// it may predate the change that caused it.
    generation: u64,
}

pub struct AccessService {
    db: Database,
    ttl: Duration,
    cache: Mutex<Cache>,
}

impl AccessService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            ttl: Duration::from_secs(30),
            cache: Mutex::new(Cache::default()),
        }
    }

    pub fn invalidate(&self, tenant_id: Uuid) {
        let mut cache = self.cache.lock().expect("access cache");
        cache.entries.remove(&tenant_id);
        cache.generation += 1;
    }

    pub async fn context(&self, identity: AuthIdentity) -> anyhow::Result<AccessContext> {
        let tenant = self.tenant_access(identity.tenant_id).await?;
        let mut tx = self.db.begin(identity.tenant_id).await?;
        let access = load_user_access(&mut tx, identity.user_id).await?;
        tx.commit().await?;
        let Some(access) = access.filter(|a| a.profile.status == "active") else {
            return Err(DomainError::forbidden("Compte inactif").into());
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
        Ok(AccessContext {
            tenant: tenant.tenant.clone(),
            engine: tenant.engine.clone(),
            roles: tenant.roles.clone(),
            subject: Arc::new(access.subject),
            profile: access.profile,
            grants,
        })
    }

    async fn tenant_access(&self, tenant_id: Uuid) -> anyhow::Result<Arc<TenantAccess>> {
        let generation = {
            let cache = self.cache.lock().expect("access cache");
            if let Some(cached) = cache.entries.get(&tenant_id)
                && cached.loaded_at.elapsed() < self.ttl
            {
                return Ok(cached.clone());
            }
            cache.generation
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let data = load_authorization_data(&mut tx).await?;
        tx.commit().await?;
        let tree = OrgTree::new(data.units)?;
        let definitions = data.roles.iter().map(|r| r.definition.clone()).collect();
        let entry = Arc::new(TenantAccess {
            tenant: data.tenant,
            roles: Arc::new(data.roles),
            engine: Arc::new(AuthorizationEngine::new(tree, definitions, data.policies)),
            loaded_at: Instant::now(),
        });
        let mut cache = self.cache.lock().expect("access cache");
        if cache.generation == generation {
            cache.entries.insert(tenant_id, entry.clone());
        }
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
