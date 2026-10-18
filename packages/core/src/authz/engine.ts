/**
 * Authorization engine: RBAC with scoped role assignments, refined by ABAC
 * policies. Pure and synchronous — callers load the inputs (roles, policies,
 * org tree, subject) and the engine decides. Every decision carries a reason
 * suitable for the audit log.
 *
 * Evaluation order for `check()`:
 *   1. tenant isolation (hard deny)
 *   2. agent ceiling: non-delegable permissions and per-run restrictions
 *   3. ABAC deny policies (deny overrides)
 *   4. sensitivity vs. subject clearance (owners always see their own data)
 *   5. RBAC grant within the assignment scope
 *   6. ABAC allow policies
 *   7. default deny
 *
 * Hierarchical position never grants anything by itself: a manager sees their
 * team's data only through a role assignment scoped to that team.
 */
import { evaluateCondition, type Condition, type EvaluationEnv } from './conditions';
import type { OrgTree } from './org-tree';
import {
  matchesPermission,
  NON_DELEGABLE_PERMISSIONS,
  type Permission,
  type PermissionPattern,
} from './permissions';
import { sensitivityRank, type Sensitivity } from './sensitivity';

export type AssignmentScope =
  | { type: 'tenant' }
  | { type: 'org_unit'; orgUnitId: string; includeDescendants: boolean }
  | { type: 'self' };

export interface RoleDefinition {
  id: string;
  key: string;
  name: string;
  permissions: readonly PermissionPattern[];
}

export interface RoleAssignment {
  roleId: string;
  scope: AssignmentScope;
  expiresAt?: Date | null;
}

export interface Subject {
  tenantId: string;
  userId: string;
  /** Org units the user belongs to (organisational context, not a grant). */
  orgUnitIds: readonly string[];
  assignments: readonly RoleAssignment[];
  /** Highest sensitivity level the user may read. Defaults to `internal`. */
  clearance?: Sensitivity;
  attributes?: Record<string, unknown>;
}

/** Who is performing the action: the user directly, or an agent on their behalf. */
export interface Actor {
  type: 'user' | 'agent';
  runId?: string;
  /**
   * For agents only: an optional allow-list narrowing what the run may use
   * (sub-agents, restricted missions). It can only narrow, never extend.
   */
  allowedPermissions?: ReadonlySet<Permission>;
  channel?: string;
}

export interface ResourceRef {
  type: string;
  id?: string;
  tenantId: string;
  orgUnitId?: string | null;
  ownerUserId?: string | null;
  sensitivity?: Sensitivity;
  attributes?: Record<string, unknown>;
}

export interface AbacPolicy {
  id: string;
  description: string;
  effect: 'allow' | 'deny';
  permissions: readonly PermissionPattern[];
  condition: Condition;
}

export type DataScope =
  | { kind: 'none' }
  | { kind: 'all' }
  | {
      kind: 'restricted';
      orgUnitIds: ReadonlySet<string>;
      /** Resources owned by these users are in scope (from `self` assignments). */
      ownerUserIds: ReadonlySet<string>;
    };

export interface Decision {
  allowed: boolean;
  reason: string;
  policyId?: string;
}

export const DEFAULT_CLEARANCE: Sensitivity = 'internal';

const USER_ACTOR: Actor = { type: 'user' };

export interface AuthorizationEngineOptions {
  orgTree: OrgTree;
  roles: readonly RoleDefinition[];
  policies?: readonly AbacPolicy[];
  now?: () => Date;
}

export class AuthorizationEngine {
  private readonly orgTree: OrgTree;
  private readonly roles: Map<string, RoleDefinition>;
  private readonly policies: readonly AbacPolicy[];
  private readonly now: () => Date;

  constructor(options: AuthorizationEngineOptions) {
    this.orgTree = options.orgTree;
    this.roles = new Map(options.roles.map((r) => [r.id, r]));
    this.policies = options.policies ?? [];
    this.now = options.now ?? (() => new Date());
  }

  /**
   * Scope in which `subject` holds `permission`. Used to pre-filter queries
   * (SQL WHERE clauses, connector filters). ABAC refinements still apply
   * per resource through `check()`.
   */
  dataScope(subject: Subject, permission: Permission, actor: Actor = USER_ACTOR): DataScope {
    if (this.agentCeilingDenies(permission, actor)) return { kind: 'none' };

    const orgUnitIds = new Set<string>();
    const ownerUserIds = new Set<string>();
    let granted = false;
    const now = this.now();

    for (const assignment of subject.assignments) {
      if (assignment.expiresAt && assignment.expiresAt <= now) continue;
      const role = this.roles.get(assignment.roleId);
      if (!role || !role.permissions.some((p) => matchesPermission(p, permission))) continue;

      granted = true;
      const scope = assignment.scope;
      if (scope.type === 'tenant') return { kind: 'all' };
      if (scope.type === 'self') {
        ownerUserIds.add(subject.userId);
      } else if (scope.includeDescendants) {
        for (const id of this.orgTree.subtree(scope.orgUnitId)) orgUnitIds.add(id);
      } else if (this.orgTree.get(scope.orgUnitId)) {
        orgUnitIds.add(scope.orgUnitId);
      }
    }

    if (!granted) return { kind: 'none' };
    return { kind: 'restricted', orgUnitIds, ownerUserIds };
  }

  /** True if the subject holds the permission in at least some scope. */
  has(subject: Subject, permission: Permission, actor: Actor = USER_ACTOR): boolean {
    return this.dataScope(subject, permission, actor).kind !== 'none';
  }

  check(
    subject: Subject,
    permission: Permission,
    resource: ResourceRef,
    actor: Actor = USER_ACTOR,
    context: Record<string, unknown> = {},
  ): Decision {
    if (resource.tenantId !== subject.tenantId) {
      return { allowed: false, reason: 'Ressource d’une autre entreprise' };
    }
    if (this.agentCeilingDenies(permission, actor)) {
      return { allowed: false, reason: `Permission ${permission} non délégable à l’agent` };
    }

    const env = this.buildEnv(subject, resource, actor, context);
    const applicable = this.policies.filter((p) =>
      p.permissions.some((pattern) => matchesPermission(pattern, permission)),
    );

    for (const policy of applicable) {
      if (policy.effect === 'deny' && evaluateCondition(policy.condition, env)) {
        return { allowed: false, reason: policy.description, policyId: policy.id };
      }
    }

    const isOwner = !!resource.ownerUserId && resource.ownerUserId === subject.userId;
    if (resource.sensitivity && !isOwner) {
      const clearance = subject.clearance ?? DEFAULT_CLEARANCE;
      if (sensitivityRank(resource.sensitivity) > sensitivityRank(clearance)) {
        return {
          allowed: false,
          reason: `Niveau de sensibilité ${resource.sensitivity} supérieur à l’habilitation ${clearance}`,
        };
      }
    }

    const scope = this.dataScope(subject, permission, actor);
    if (scopeIncludes(scope, resource)) {
      return { allowed: true, reason: `Rôle accordant ${permission} sur ce périmètre` };
    }

    for (const policy of applicable) {
      if (policy.effect === 'allow' && evaluateCondition(policy.condition, env)) {
        return { allowed: true, reason: policy.description, policyId: policy.id };
      }
    }

    return {
      allowed: false,
      reason:
        scope.kind === 'none'
          ? `Permission ${permission} non accordée`
          : `Ressource hors du périmètre accordé pour ${permission}`,
    };
  }

  private agentCeilingDenies(permission: Permission, actor: Actor): boolean {
    if (actor.type !== 'agent') return false;
    if (NON_DELEGABLE_PERMISSIONS.has(permission)) return true;
    return actor.allowedPermissions !== undefined && !actor.allowedPermissions.has(permission);
  }

  private buildEnv(
    subject: Subject,
    resource: ResourceRef,
    actor: Actor,
    context: Record<string, unknown>,
  ): EvaluationEnv {
    return {
      subject: {
        userId: subject.userId,
        orgUnitIds: [...subject.orgUnitIds],
        clearanceRank: sensitivityRank(subject.clearance ?? DEFAULT_CLEARANCE),
        attributes: subject.attributes ?? {},
      },
      resource: {
        type: resource.type,
        id: resource.id,
        orgUnitId: resource.orgUnitId ?? undefined,
        ownerUserId: resource.ownerUserId ?? undefined,
        sensitivityRank: resource.sensitivity ? sensitivityRank(resource.sensitivity) : undefined,
        attributes: resource.attributes ?? {},
      },
      context: { ...context, actorType: actor.type, channel: actor.channel },
    };
  }
}

/** Whether a resource falls inside a data scope (RBAC part only). */
export function scopeIncludes(
  scope: DataScope,
  resource: Pick<ResourceRef, 'orgUnitId' | 'ownerUserId'>,
): boolean {
  if (scope.kind === 'all') return true;
  if (scope.kind === 'none') return false;
  if (resource.orgUnitId && scope.orgUnitIds.has(resource.orgUnitId)) return true;
  return !!resource.ownerUserId && scope.ownerUserIds.has(resource.ownerUserId);
}
