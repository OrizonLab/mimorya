import { describe, expect, it } from 'vitest';
import { DEMO_UNITS, DEMO_USERS, demoAuthorizationEngine, demoOrgTree, demoSubject } from './demo';
import { canWriteMemory, memoryReadFilter } from './memory';

const engine = demoAuthorizationEngine();
const tree = demoOrgTree();

describe('memory visibility', () => {
  it('a technician reads personal memory plus their team, department and company knowledge', () => {
    const filter = memoryReadFilter(engine, tree, demoSubject(DEMO_USERS.lucas));
    expect(filter.ownerUserId).toBe(DEMO_USERS.lucas);
    expect(filter.shared).toBe(true);
    expect(filter.orgUnitIds).toEqual(
      new Set([DEMO_UNITS.teamNorth, DEMO_UNITS.technical, DEMO_UNITS.company]),
    );
  });

  it('a technician cannot read the other team’s knowledge', () => {
    const filter = memoryReadFilter(engine, tree, demoSubject(DEMO_USERS.lucas));
    expect(filter.orgUnitIds !== 'all' && filter.orgUnitIds.has(DEMO_UNITS.teamSouth)).toBe(false);
  });

  it('only shared spaces within the write scope are writable', () => {
    const julien = demoSubject(DEMO_USERS.julien);
    expect(canWriteMemory(engine, julien, { scope: 'user' })).toBe(true);
    expect(
      canWriteMemory(engine, julien, { scope: 'org_unit', orgUnitId: DEMO_UNITS.teamNorth }),
    ).toBe(true);
    expect(
      canWriteMemory(engine, julien, { scope: 'org_unit', orgUnitId: DEMO_UNITS.teamSouth }),
    ).toBe(false);
    expect(canWriteMemory(engine, julien, { scope: 'tenant' })).toBe(false);

    const lucas = demoSubject(DEMO_USERS.lucas);
    expect(
      canWriteMemory(engine, lucas, { scope: 'org_unit', orgUnitId: DEMO_UNITS.teamNorth }),
    ).toBe(false);
    const claire = demoSubject(DEMO_USERS.claire);
    expect(canWriteMemory(engine, claire, { scope: 'tenant' })).toBe(true);
  });
});
