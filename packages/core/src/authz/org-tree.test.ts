import { describe, expect, it } from 'vitest';
import { DEMO_UNITS, demoOrgTree } from '../demo';
import { OrgTree } from './org-tree';

describe('OrgTree', () => {
  const tree = demoOrgTree();

  it('computes subtrees of unlimited depth', () => {
    expect(tree.subtree(DEMO_UNITS.technical)).toEqual(
      new Set([DEMO_UNITS.technical, DEMO_UNITS.teamNorth, DEMO_UNITS.teamSouth]),
    );
    expect(tree.subtree(DEMO_UNITS.company).size).toBe(6);
    expect(tree.subtree('unknown').size).toBe(0);
  });

  it('lists ancestors from parent to root and builds a readable path', () => {
    expect(tree.ancestors(DEMO_UNITS.teamNorth).map((u) => u.id)).toEqual([
      DEMO_UNITS.technical,
      DEMO_UNITS.company,
    ]);
    expect(tree.pathLabel(DEMO_UNITS.teamNorth)).toBe(
      'Néo Maintenance › Service technique › Équipe Nord',
    );
  });

  it('answers subtree membership', () => {
    expect(tree.isInSubtree(DEMO_UNITS.technical, DEMO_UNITS.teamSouth)).toBe(true);
    expect(tree.isInSubtree(DEMO_UNITS.teamSouth, DEMO_UNITS.technical)).toBe(false);
    expect(tree.isInSubtree(DEMO_UNITS.admin, DEMO_UNITS.admin)).toBe(true);
  });

  it('supports deep hierarchies', () => {
    const units = Array.from({ length: 50 }, (_, i) => ({
      id: `u${i}`,
      parentId: i === 0 ? null : `u${i - 1}`,
      name: `Niveau ${i}`,
      type: 'team' as const,
    }));
    const deep = new OrgTree(units);
    expect(deep.subtree('u10').size).toBe(40);
    expect(deep.ancestors('u49')).toHaveLength(49);
  });

  it('rejects unknown parents, duplicates and cycles', () => {
    expect(() => new OrgTree([{ id: 'a', parentId: 'x', name: 'A', type: 'team' }])).toThrow(
      /unknown parent/,
    );
    expect(
      () =>
        new OrgTree([
          { id: 'a', parentId: null, name: 'A', type: 'team' },
          { id: 'a', parentId: null, name: 'A', type: 'team' },
        ]),
    ).toThrow(/Duplicate/);
    expect(
      () =>
        new OrgTree([
          { id: 'a', parentId: 'b', name: 'A', type: 'team' },
          { id: 'b', parentId: 'a', name: 'B', type: 'team' },
        ]),
    ).toThrow(/Cycle/);
  });
});
