import { AuthorizationEngine } from '../authz/engine';
import type { Subject } from '../authz/types';
import { OrgTree } from '../authz/org-tree';
import { DEMO_TENANT } from './ids';
import { DEMO_ORG_UNITS } from './org-units';
import { DEMO_POLICIES } from './policies';
import { DEMO_ROLE_DEFINITIONS } from './roles';
import { DEMO_USER_LIST } from './users';

export * from './ids';
export * from './org-units';
export * from './roles';
export * from './users';
export * from './policies';

export function demoOrgTree(): OrgTree {
  return new OrgTree(DEMO_ORG_UNITS);
}

export function demoAuthorizationEngine(now?: () => Date): AuthorizationEngine {
  return new AuthorizationEngine({
    orgTree: demoOrgTree(),
    roles: DEMO_ROLE_DEFINITIONS,
    policies: DEMO_POLICIES,
    ...(now ? { now } : {}),
  });
}

export function demoSubject(userId: string): Subject {
  const user = DEMO_USER_LIST.find((u) => u.id === userId);
  if (!user) throw new Error(`Unknown demo user ${userId}`);
  return {
    tenantId: DEMO_TENANT.id,
    userId: user.id,
    orgUnitIds: [user.primaryUnitId],
    assignments: user.assignments,
    clearance: user.clearance,
    attributes: user.attributes,
  };
}
