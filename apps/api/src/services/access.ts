/**
 * Builds the authorization context of a request: the tenant's engine (cached
 * briefly per tenant) and the subject of the authenticated user, always
 * loaded from the database — never from the client.
 */
import {
  AuthorizationEngine,
  ForbiddenError,
  OrgTree,
  type AssignmentScope,
  type Permission,
  type Subject,
} from '@mimorya/core';
import {
  loadAuthorizationData,
  loadUserAccess,
  withTenant,
  type AuthorizationData,
  type Database,
  type UserProfile,
} from '@mimorya/db';

export interface AuthIdentity {
  tenantId: string;
  userId: string;
  sessionId: string;
}

export interface AccessContext {
  tenant: AuthorizationData['tenant'];
  engine: AuthorizationEngine;
  orgTree: OrgTree;
  roles: AuthorizationData['roles'];
  subject: Subject;
  profile: UserProfile;
  /** Human-readable grants, e.g. "Responsable d’équipe — Service technique et sous-unités". */
  grants: string[];
  require(permission: Permission): void;
}

interface CacheEntry {
  data: AuthorizationData;
  engine: AuthorizationEngine;
  orgTree: OrgTree;
  loadedAt: number;
}

export class AccessService {
  private readonly cache = new Map<string, CacheEntry>();

  constructor(
    private readonly db: Database,
    private readonly ttlMs = 30_000,
  ) {}

  invalidate(tenantId: string): void {
    this.cache.delete(tenantId);
  }

  async context(identity: AuthIdentity): Promise<AccessContext> {
    const tenantEntry = await this.tenantEntry(identity.tenantId);
    const access = await withTenant(this.db, identity.tenantId, (tx) =>
      loadUserAccess(tx, identity.userId),
    );
    if (!access || access.profile.status !== 'active') throw new ForbiddenError('Compte inactif');
    const { engine, orgTree, data } = tenantEntry;
    const subject = access.subject;
    return {
      tenant: data.tenant,
      engine,
      orgTree,
      roles: data.roles,
      subject,
      profile: access.profile,
      grants: access.assignments.map((a) => `${a.roleName} — ${describeScope(a.scope, orgTree)}`),
      require(permission) {
        if (!engine.has(subject, permission)) {
          throw new ForbiddenError(`Permission requise : ${permission}`);
        }
      },
    };
  }

  private async tenantEntry(tenantId: string): Promise<CacheEntry> {
    const cached = this.cache.get(tenantId);
    if (cached && Date.now() - cached.loadedAt < this.ttlMs) return cached;
    const data = await withTenant(this.db, tenantId, (tx) => loadAuthorizationData(tx));
    const orgTree = new OrgTree(data.units);
    const entry = {
      data,
      orgTree,
      engine: new AuthorizationEngine({ orgTree, roles: data.roles, policies: data.policies }),
      loadedAt: Date.now(),
    };
    this.cache.set(tenantId, entry);
    return entry;
  }
}

export function describeScope(scope: AssignmentScope, orgTree: OrgTree): string {
  if (scope.type === 'tenant') return 'toute l’entreprise';
  if (scope.type === 'self') return 'ses propres données';
  const unit = orgTree.get(scope.orgUnitId)?.name ?? 'unité inconnue';
  return scope.includeDescendants ? `${unit} et sous-unités` : unit;
}
