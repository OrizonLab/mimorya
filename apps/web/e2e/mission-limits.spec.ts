// A company's own mission limits (ADR-093): Paul sets them in « Modèle
// d'IA », read back once the page is reloaded; a mission Emma entrusts
// then keeps them, whose model takes its time, and is stopped. Back to the
// platform's limits, the page says so. Whatever happens, the platform's
// limits are set again and Emma's missions under way stopped: the next
// files expect them (`missions.spec.ts`). The rules themselves are tested
// on the API (`crates/api/tests/mission_settings.rs`).
import type { Page } from '@playwright/test';
import type { MissionList, NewMission } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
/** A title the server's model waits on (`e2e_server/missions.rs`). */
const TITLE = 'Relevé aux limites de l’entreprise (à arrêter)';

/** Who the test signed in, for its cleanup. */
const left: { paul?: Page; emma?: Page } = {};

test.afterEach(async () => {
  const { paul, emma } = left;
  if (paul) {
    const reset = await paul.request.delete('/api/admin/mission-settings', { headers: WRITE });
    expect(reset.status()).toBe(200);
  }
  if (emma) {
    const listed = await emma.request.get('/api/missions');
    const { missions } = (await listed.json()) as MissionList;
    for (const mission of missions) {
      if (mission.status !== 'in_progress' && mission.status !== 'awaiting_approval') continue;
      const stopped = await emma.request.post(`/api/missions/${mission.id}/stop`, {
        headers: WRITE,
      });
      expect(stopped.status()).toBe(200);
    }
  }
  left.paul = left.emma = undefined;
});

test('a mission keeps the limits its company set', async ({ page, newContext }) => {
  await openAdmin(page, 'Modèle d’IA');
  left.paul = page;
  const form = page.getByRole('form', { name: 'Limites des missions' });
  const stages = form.getByLabel('Étapes au plus par mission');
  const cost = form.getByLabel('Coût au plus par mission (USD)');
  await expect(stages).toHaveValue('6');
  await expect(cost).toHaveValue('12');
  await expect(form.getByText('Limites de la plateforme, faute de')).toBeVisible();
  await stages.fill('3');
  await cost.fill('5');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  // Anchored and case-sensitive: the platform's message holds these words too.
  await expect(form.getByText(/^Limites propres à votre entreprise\./)).toBeVisible();

  // Read back from the API once the page is reloaded.
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await expect(stages).toHaveValue('3');
  await expect(cost).toHaveValue('5');
  await noError(page);

  // Emma's mission, entrusted now, keeps them.
  const emma = await (await newContext()).newPage();
  await signIn(emma, demo.emma);
  left.emma = emma;
  const entrusted = await emma.request.post('/api/missions', {
    headers: WRITE,
    data: {
      title: TITLE,
      objective: 'Relis les interventions de la semaine.',
      successCriteria: ['Chaque intervention est relue'],
    } satisfies NewMission,
  });
  expect(entrusted.status()).toBe(201);
  await emma.goto('/missions');
  const mission = emma.getByRole('article', { name: TITLE });
  await expect(mission.getByText(/étape 1 sur 3 au plus/)).toBeVisible();
  const listed = (await (await emma.request.get('/api/missions')).json()) as MissionList;
  const kept = listed.missions.find((m) => m.title === TITLE);
  expect(kept?.maxCostMicros).toBe(5_000_000);
  await noError(emma);

  // Back to the platform's, for the next missions.
  page.once('dialog', (dialog) => void dialog.accept());
  await form.getByRole('button', { name: 'Revenir aux limites de la plateforme' }).click();
  await expect(form.getByText('Limites de la plateforme, faute de')).toBeVisible();
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await expect(stages).toHaveValue('6');
  await expect(cost).toHaveValue('12');
  await expect(
    form.getByRole('button', { name: 'Revenir aux limites de la plateforme' }),
  ).toHaveCount(0);
  await noError(page);
});
