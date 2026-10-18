import { AuthorizationEngine, type Subject } from '../authz/engine';
import { OrgTree } from '../authz/org-tree';
import {
  DEMO_ORG_UNITS,
  DEMO_POLICIES,
  DEMO_ROLE_DEFINITIONS,
  DEMO_TENANT,
  DEMO_USER_LIST,
} from './organization';

export * from './organization';

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
