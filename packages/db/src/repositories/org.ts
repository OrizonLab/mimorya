import type {
  AbacPolicy,
  AssignmentScope,
  OrgUnit,
  PermissionPattern,
  RoleAssignment,
  RoleDefinition,
  Sensitivity,
  Subject,
} from '@mimorya/core';
import { asc, eq } from 'drizzle-orm';
import type { Executor } from '../client';
import {
  accessPolicies,
  orgMemberships,
  orgUnits,
  roleAssignments,
  roles,
  tenants,
  users,
} from '../schema';

export interface AuthorizationData {
  tenant: { id: string; name: string; settings: Record<string, unknown> };
  units: OrgUnit[];
  roles: Array<RoleDefinition & { description: string; isSystem: boolean }>;
  policies: AbacPolicy[];
}

/** Everything the authorization engine needs for one tenant (RLS-scoped). */
export async function loadAuthorizationData(tx: Executor): Promise<AuthorizationData> {
  const [tenant] = await tx.select().from(tenants);
  if (!tenant) throw new Error('Tenant context missing');
  const unitRows = await tx.select().from(orgUnits).orderBy(asc(orgUnits.createdAt));
  const roleRows = await tx.select().from(roles).orderBy(asc(roles.name));
  const policyRows = await tx.select().from(accessPolicies).where(eq(accessPolicies.enabled, true));
  return {
    tenant: { id: tenant.id, name: tenant.name, settings: tenant.settings },
    units: unitRows.map((u) => ({
      id: u.id,
      parentId: u.parentId,
      name: u.name,
      type: u.type as OrgUnit['type'],
      leadUserId: u.leadUserId,
    })),
    roles: roleRows.map((r) => ({
      id: r.id,
      key: r.key,
      name: r.name,
      description: r.description,
      isSystem: r.isSystem,
      permissions: r.permissions as PermissionPattern[],
    })),
    policies: policyRows.map((p) => ({
      id: p.id,
      description: p.description,
      effect: p.effect as AbacPolicy['effect'],
      permissions: p.permissions as PermissionPattern[],
      condition: p.condition,
    })),
  };
}

export interface UserProfile {
  id: string;
  tenantId: string;
  email: string;
  displayName: string;
  jobTitle: string | null;
  status: string;
  locale: string;
  primaryUnitId: string | null;
}

export interface UserAccess {
  profile: UserProfile;
  subject: Subject;
  assignments: Array<RoleAssignment & { roleName: string }>;
}

function toScope(row: typeof roleAssignments.$inferSelect): AssignmentScope {
  if (row.scopeType === 'org_unit' && row.scopeOrgUnitId) {
    return {
      type: 'org_unit',
      orgUnitId: row.scopeOrgUnitId,
      includeDescendants: row.includeDescendants,
    };
  }
  return row.scopeType === 'tenant' ? { type: 'tenant' } : { type: 'self' };
}

/** Loads a user with memberships and role assignments, ready for the engine. */
export async function loadUserAccess(tx: Executor, userId: string): Promise<UserAccess | null> {
  const [user] = await tx.select().from(users).where(eq(users.id, userId));
  if (!user) return null;
  const memberships = await tx
    .select()
    .from(orgMemberships)
    .where(eq(orgMemberships.userId, userId));
  const assignmentRows = await tx
    .select({ assignment: roleAssignments, roleName: roles.name })
    .from(roleAssignments)
    .innerJoin(roles, eq(roles.id, roleAssignments.roleId))
    .where(eq(roleAssignments.userId, userId));

  const ordered = [...memberships].sort((a, b) => Number(b.isPrimary) - Number(a.isPrimary));
  const assignments = assignmentRows.map(({ assignment, roleName }) => ({
    roleId: assignment.roleId,
    scope: toScope(assignment),
    expiresAt: assignment.expiresAt,
    roleName,
  }));
  return {
    profile: {
      id: user.id,
      tenantId: user.tenantId,
      email: user.email,
      displayName: user.displayName,
      jobTitle: user.jobTitle,
      status: user.status,
      locale: user.locale,
      primaryUnitId: ordered[0]?.orgUnitId ?? null,
    },
    subject: {
      tenantId: user.tenantId,
      userId: user.id,
      orgUnitIds: ordered.map((m) => m.orgUnitId),
      assignments,
      clearance: user.clearance as Sensitivity,
      attributes: user.attributes,
    },
    assignments,
  };
}

export interface DirectoryEntry {
  id: string;
  email: string;
  displayName: string;
  jobTitle: string | null;
  status: string;
  primaryUnitId: string | null;
  lastLoginAt: Date | null;
}

export async function listUsers(tx: Executor): Promise<DirectoryEntry[]> {
  const rows = await tx
    .select({
      id: users.id,
      email: users.email,
      displayName: users.displayName,
      jobTitle: users.jobTitle,
      status: users.status,
      lastLoginAt: users.lastLoginAt,
    })
    .from(users)
    .orderBy(asc(users.displayName));
  const memberships = await tx
    .select()
    .from(orgMemberships)
    .where(eq(orgMemberships.isPrimary, true));
  const primary = new Map(memberships.map((m) => [m.userId, m.orgUnitId]));
  return rows.map((r) => ({ ...r, primaryUnitId: primary.get(r.id) ?? null }));
}

export async function listRoleAssignments(tx: Executor) {
  return tx
    .select({
      userId: roleAssignments.userId,
      roleId: roleAssignments.roleId,
      roleName: roles.name,
      scopeType: roleAssignments.scopeType,
      scopeOrgUnitId: roleAssignments.scopeOrgUnitId,
      includeDescendants: roleAssignments.includeDescendants,
      expiresAt: roleAssignments.expiresAt,
    })
    .from(roleAssignments)
    .innerJoin(roles, eq(roles.id, roleAssignments.roleId));
}
