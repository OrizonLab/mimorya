/**
 * Demo tenant, accounts and access model: users, org units, memberships,
 * roles, scoped assignments and ABAC policies (FICTIONAL, @mimorya/core/demo).
 */
import {
  DEMO_ORG_UNITS,
  DEMO_POLICIES,
  DEMO_ROLE_DEFINITIONS,
  DEMO_TENANT,
  DEMO_USER_LIST,
  DEMO_USERS,
} from '@mimorya/core/demo';
import type { Transaction } from '../client';
import {
  accessPolicies,
  orgMemberships,
  orgUnits,
  roleAssignments,
  roles,
  tenants,
  users,
} from '../schema';
import type { SeedOptions } from './options';

export async function insertDemoOrganization(tx: Transaction, options: SeedOptions): Promise<void> {
  const tenantId = DEMO_TENANT.id;
  await tx.insert(tenants).values({
    id: tenantId,
    slug: DEMO_TENANT.slug,
    name: DEMO_TENANT.name,
    settings: { demo: true, defaultAutonomyLevel: 'execute_with_approval' },
  });

  await tx.insert(users).values(
    DEMO_USER_LIST.map((u) => ({
      id: u.id,
      tenantId,
      email: u.email,
      displayName: u.displayName,
      jobTitle: u.jobTitle,
      passwordHash: options.passwordHash,
      clearance: u.clearance,
      attributes: u.attributes,
    })),
  );

  // Parents are listed before children in the fixture.
  for (const unit of DEMO_ORG_UNITS) {
    await tx.insert(orgUnits).values({ ...unit, tenantId });
  }

  await tx.insert(orgMemberships).values(
    DEMO_USER_LIST.map((u) => ({
      tenantId,
      userId: u.id,
      orgUnitId: u.primaryUnitId,
      isPrimary: true,
    })),
  );

  await tx.insert(roles).values(
    DEMO_ROLE_DEFINITIONS.map((r) => ({
      id: r.id,
      tenantId,
      key: r.key,
      name: r.name,
      description: r.description,
      permissions: [...r.permissions],
      isSystem: r.isSystem,
    })),
  );

  await tx.insert(roleAssignments).values(
    DEMO_USER_LIST.flatMap((u) =>
      u.assignments.map((a) => ({
        tenantId,
        userId: u.id,
        roleId: a.roleId,
        scopeType: a.scope.type,
        scopeOrgUnitId: a.scope.type === 'org_unit' ? a.scope.orgUnitId : null,
        includeDescendants: a.scope.type === 'org_unit' ? a.scope.includeDescendants : true,
        grantedByUserId: DEMO_USERS.paul,
      })),
    ),
  );

  await tx.insert(accessPolicies).values(
    DEMO_POLICIES.map((p) => ({
      id: p.id,
      tenantId,
      description: p.description,
      effect: p.effect,
      permissions: [...p.permissions],
      condition: p.condition,
    })),
  );
}
