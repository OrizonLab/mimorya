import { describe, expect, it } from 'vitest';
import {
  DEMO_ROLES,
  DEMO_TENANT,
  DEMO_UNITS,
  DEMO_USERS,
  demoAuthorizationEngine,
  demoSubject,
} from '../demo';
import { scopeIncludes, type ResourceRef, type Subject } from './engine';

const engine = demoAuthorizationEngine();

function intervention(
  technicianId: string,
  orgUnitId: string,
  status = 'in_progress',
): ResourceRef {
  return {
    type: 'intervention',
    tenantId: DEMO_TENANT.id,
    ownerUserId: technicianId,
    orgUnitId,
    attributes: { status },
  };
}

const lucasJob = intervention(DEMO_USERS.lucas, DEMO_UNITS.teamNorth);
const hugoJob = intervention(DEMO_USERS.hugo, DEMO_UNITS.teamSouth);

describe('AuthorizationEngine — demo organisation', () => {
  it('a technician only sees their own interventions', () => {
    const lucas = demoSubject(DEMO_USERS.lucas);
    expect(engine.check(lucas, 'intervention:read', lucasJob).allowed).toBe(true);
    expect(engine.check(lucas, 'intervention:read', hugoJob).allowed).toBe(false);
    const emmaJob = intervention(DEMO_USERS.emma, DEMO_UNITS.teamNorth);
    expect(engine.check(lucas, 'intervention:read', emmaJob).allowed).toBe(false);
  });

  it('a team lead sees their team, not the other team', () => {
    const julien = demoSubject(DEMO_USERS.julien);
    expect(engine.check(julien, 'intervention:read', lucasJob).allowed).toBe(true);
    const decision = engine.check(julien, 'intervention:read', hugoJob);
    expect(decision.allowed).toBe(false);
    expect(decision.reason).toMatch(/hors du périmètre/);
  });

  it('the technical manager sees the whole department subtree', () => {
    const marc = demoSubject(DEMO_USERS.marc);
    const scope = engine.dataScope(marc, 'intervention:read');
    expect(scope.kind).toBe('restricted');
    expect(scopeIncludes(scope, lucasJob)).toBe(true);
    expect(scopeIncludes(scope, hugoJob)).toBe(true);
    expect(engine.check(marc, 'intervention:read', hugoJob).allowed).toBe(true);
  });

  it('the director sees everything in the tenant', () => {
    const claire = demoSubject(DEMO_USERS.claire);
    expect(engine.dataScope(claire, 'intervention:read')).toEqual({ kind: 'all' });
  });

  it('the technical administrator manages the workspace but has no business data access', () => {
    const paul = demoSubject(DEMO_USERS.paul);
    expect(engine.has(paul, 'user:manage')).toBe(true);
    expect(engine.check(paul, 'intervention:read', lucasJob).allowed).toBe(false);
  });

  it('applies the ABAC deny policy for administrative staff', () => {
    const isabelle = demoSubject(DEMO_USERS.isabelle);
    const open = engine.check(isabelle, 'intervention:read', lucasJob);
    expect(open.allowed).toBe(false);
    expect(open.policyId).toBeDefined();
    const done = intervention(DEMO_USERS.lucas, DEMO_UNITS.teamNorth, 'completed');
    expect(engine.check(isabelle, 'intervention:read', done).allowed).toBe(true);
  });

  it('enforces tenant isolation before anything else', () => {
    const claire = demoSubject(DEMO_USERS.claire);
    const foreign = { ...lucasJob, tenantId: 'other-tenant' };
    const decision = engine.check(claire, 'intervention:read', foreign);
    expect(decision.allowed).toBe(false);
    expect(decision.reason).toMatch(/autre entreprise/);
  });

  it('enforces sensitivity clearance, except for the owner', () => {
    const lucas = demoSubject(DEMO_USERS.lucas); // clearance: internal
    const marc = demoSubject(DEMO_USERS.marc); // clearance: confidential
    const confidential: ResourceRef = { ...lucasJob, sensitivity: 'confidential' };
    expect(engine.check(lucas, 'intervention:read', confidential).allowed).toBe(true);
    expect(engine.check(marc, 'intervention:read', confidential).allowed).toBe(true);
    const restricted: ResourceRef = { ...hugoJob, sensitivity: 'restricted' };
    expect(engine.check(marc, 'intervention:read', restricted).allowed).toBe(false);
  });

  it('ignores expired assignments', () => {
    const now = new Date('2026-01-10T00:00:00Z');
    const timed = demoAuthorizationEngine(() => now);
    const temp: Subject = {
      ...demoSubject(DEMO_USERS.lucas),
      assignments: [
        {
          roleId: DEMO_ROLES.teamLead,
          scope: { type: 'org_unit', orgUnitId: DEMO_UNITS.teamSouth, includeDescendants: true },
          expiresAt: new Date('2026-01-01T00:00:00Z'),
        },
      ],
    };
    expect(timed.check(temp, 'intervention:read', hugoJob).allowed).toBe(false);
  });
});

describe('AuthorizationEngine — agents acting on behalf of users', () => {
  it('never exercises non-delegable permissions, whatever the user holds', () => {
    const paul = demoSubject(DEMO_USERS.paul);
    const agent = { type: 'agent' as const, runId: 'run-1' };
    expect(engine.has(paul, 'role:manage')).toBe(true);
    expect(engine.has(paul, 'role:manage', agent)).toBe(false);
    expect(engine.has(paul, 'user:manage', agent)).toBe(false);
    expect(engine.has(demoSubject(DEMO_USERS.marc), 'approval:decide', agent)).toBe(false);
  });

  it('inherits the user’s data scope and can be narrowed, never extended', () => {
    const julien = demoSubject(DEMO_USERS.julien);
    const agent = { type: 'agent' as const, runId: 'run-2' };
    expect(engine.check(julien, 'intervention:read', lucasJob, agent).allowed).toBe(true);
    expect(engine.check(julien, 'intervention:read', hugoJob, agent).allowed).toBe(false);

    const narrowed = {
      type: 'agent' as const,
      runId: 'run-3',
      allowedPermissions: new Set(['planning:read' as const]),
    };
    expect(engine.check(julien, 'intervention:read', lucasJob, narrowed).allowed).toBe(false);

    const lucas = demoSubject(DEMO_USERS.lucas);
    const widened = {
      type: 'agent' as const,
      runId: 'run-4',
      allowedPermissions: new Set(['planning:apply' as const]),
    };
    expect(engine.has(lucas, 'planning:apply', widened)).toBe(false);
  });
});
