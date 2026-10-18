import { memoryReadFilter, sensitivityRank } from '@mimorya/core';
import {
  DEMO_TENANT,
  DEMO_UNITS,
  DEMO_USERS,
  demoAuthorizationEngine,
  demoOrgTree,
  demoSubject,
} from '@mimorya/core/demo';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createDatabase, withTenant, type DatabaseHandle } from './client';
import { createMemory, listVisibleMemory, searchMemory, toOrTsQuery } from './repositories/memory';
import { resetTestDatabase } from './testing';

let app: DatabaseHandle;
const engine = demoAuthorizationEngine();
const tree = demoOrgTree();

beforeAll(async () => {
  const urls = await resetTestDatabase();
  app = createDatabase(urls.app, { max: 2 });
  await withTenant(app.db, DEMO_TENANT.id, (tx) =>
    createMemory(tx, {
      tenantId: DEMO_TENANT.id,
      scope: 'org_unit',
      orgUnitId: DEMO_UNITS.teamSouth,
      kind: 'procedure',
      title: 'Retours chantier équipe Sud',
      content: 'Les retours chantier de l’équipe Sud ont lieu le mardi matin.',
      status: 'active',
      source: { type: 'manual' },
    }),
  );
});

afterAll(async () => {
  await app?.close();
});

async function visibleTitles(userId: string): Promise<string[]> {
  const subject = demoSubject(userId);
  const filter = memoryReadFilter(engine, tree, subject);
  const rows = await withTenant(app.db, DEMO_TENANT.id, (tx) =>
    listVisibleMemory(tx, filter, sensitivityRank(subject.clearance ?? 'internal')),
  );
  return rows.map((r) => r.title).sort();
}

describe('memory visibility in SQL', () => {
  it('builds OR full-text queries from natural language', () => {
    expect(toOrTsQuery('Retours chantier : équipe Nord ?')).toBe(
      'retours | chantier | équipe | nord',
    );
    expect(toOrTsQuery("a ' ; --")).toBeNull();
  });

  it('a technician sees personal, team, department and company memory — nothing else', async () => {
    expect(await visibleTitles(DEMO_USERS.lucas)).toEqual([
      'Clôture d’une intervention',
      'Horaires d’intervention',
      'Prévenance des déplacements',
      'Retours chantier équipe Nord',
    ]);
  });

  it('a manager never sees a collaborator’s personal memory', async () => {
    const titles = await visibleTitles(DEMO_USERS.marc);
    expect(titles).toContain('Format des synthèses');
    expect(titles).toContain('Retours chantier équipe Sud');
    expect(titles).not.toContain('Prévenance des déplacements');
  });

  it('ranks full-text matches within the visible set only', async () => {
    const subject = demoSubject(DEMO_USERS.lucas);
    const hits = await withTenant(app.db, DEMO_TENANT.id, (tx) =>
      searchMemory(
        tx,
        memoryReadFilter(engine, tree, subject),
        'Quand ont lieu les retours chantier ?',
        {
          limit: 5,
          clearanceRank: sensitivityRank('internal'),
        },
      ),
    );
    const titles = hits.map((h) => h.title);
    expect(titles[0]).toBe('Retours chantier équipe Nord');
    expect(titles).not.toContain('Retours chantier équipe Sud');
  });
});
