/** Memory rights through the API: personal memory stays personal. */
import { DEMO_USERS } from '@mimorya/core/demo';
import { describe, expect, it } from 'vitest';
import { json, useTestApi } from './harness';

const api = useTestApi();
const { as, login } = api;

describe('memory rights', () => {
  it('lets users see and erase their own memory, never other people’s', async () => {
    const lucas = as(await login(DEMO_USERS.lucas));
    const marc = as(await login(DEMO_USERS.marc));
    const lucasItems = json<{ items: Array<{ id: string; title: string; canManage: boolean }> }>(
      await lucas.get('/memory'),
    ).items;
    expect(lucasItems.map((i) => i.title)).not.toContain('Format des synthèses');
    const own = lucasItems.find((i) => i.title === 'Prévenance des déplacements')!;
    expect(own.canManage).toBe(true);

    expect((await marc.delete(`/memory/${own.id}`)).statusCode).toBe(404);
    expect((await lucas.delete(`/memory/${own.id}`)).statusCode).toBe(200);
    const after = json<{ items: Array<{ id: string }> }>(await lucas.get('/memory')).items;
    expect(after.some((i) => i.id === own.id)).toBe(false);

    const shared = lucasItems.find((i) => i.title === 'Retours chantier équipe Nord')!;
    expect(shared.canManage).toBe(false);
    expect((await lucas.delete(`/memory/${shared.id}`)).statusCode).toBe(404);
  });
});
