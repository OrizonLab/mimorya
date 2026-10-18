/** Authorization data model: subjects, actors, role assignments, resources and decisions. */
import type { Condition } from './conditions';
import type { Permission, PermissionPattern } from './permissions';
import type { Sensitivity } from './sensitivity';

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
