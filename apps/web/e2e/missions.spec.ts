// Missions (ADR-089): Emma entrusts one from « Missions »; its stages go on
// in the background, the first ending on its progress note, the second on
// the answer, reviewed against its criteria, which the page shows once it
// reads the list again. The conversation shows the brief, the note and the
// word that set the second stage going. One whose review gives no verdict
// is not said to meet its criteria. At three under way, the page says so
// instead of the form; one of them, whose model takes its time, is stopped
// from the page. Whatever happens, none is left under way.
// The rules themselves are tested on the API (`crates/api/tests/missions.rs`).
import type { Page } from '@playwright/test';
import type { MissionList, NewMission } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const CARRIED_OUT = 'Suivi des interventions de la semaine';
/** A title the server's model waits on (`e2e_server/missions.rs`). */
const STOPPED_TITLE = 'Relevé du parc (à arrêter)';
const OBJECTIVE = 'Relis les interventions de la semaine et propose leur suivi.';
/** An objective whose review the server's model leaves without a verdict. */
const UNJUDGED_OBJECTIVE = 'Relis les interventions de la semaine (relecture sans verdict).';
const CRITERIA = ['Chaque intervention de la semaine est relue', 'Un suivi est proposé'];
/** What the server's model writes (`e2e_server/missions.rs`). */
const NOTE =
  'Point d’étape : les interventions de la semaine sont relues. Reste à proposer leur suivi.';
const ANSWER =
  'Synthèse de la mission : les interventions de la semaine sont relues et leur suivi est proposé à l’équipe.';
const STOPPED = 'Mission arrêtée à la demande de son auteur.';
const FULL = 'Vous avez déjà 3 missions en cours';

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/missions');
  // Not signed in: nothing was entrusted.
  if (listed.status() !== 200) return;
  const { missions } = (await listed.json()) as MissionList;
  for (const mission of missions) {
    if (mission.status !== 'in_progress' && mission.status !== 'awaiting_approval') continue;
    const stopped = await page.request.post(`/api/missions/${mission.id}/stop`, {
      headers: WRITE,
    });
    expect(stopped.status()).toBe(200);
  }
});

/** Entrusts a mission titled `title` from the page. */
async function entrust(page: Page, title: string, objective = OBJECTIVE) {
  await page.getByLabel('Intitulé').fill(title);
  await page.getByLabel('Objectif').fill(objective);
  await page.getByLabel('Critères de réussite (un par ligne, 8 au plus)').fill(CRITERIA.join('\n'));
  await page.getByRole('button', { name: 'Confier la mission' }).click();
  // Emptied once entrusted.
  await expect(page.getByLabel('Intitulé')).toHaveValue('');
  return page.getByRole('article', { name: title });
}

test('a mission goes from stage to stage to its reviewed answer', async ({ page }) => {
  test.setTimeout(150_000);
  await signIn(page, demo.emma);
  await page.getByRole('link', { name: 'Missions', exact: true }).click();
  const mission = await entrust(page, CARRIED_OUT);
  await expect(mission.getByText('Un suivi est proposé')).toBeVisible();

  // Followed by the page, which reads the list again while it goes on.
  await expect(mission.getByText('Terminée', { exact: true })).toBeVisible({ timeout: 120_000 });
  await page.reload();
  await expect(mission.getByText('Terminée', { exact: true })).toBeVisible();
  await expect(mission.getByText(/étape 2 sur 6 au plus/)).toBeVisible();
  const result = mission.getByRole('region', { name: 'Résultat' });
  await expect(result.getByText(ANSWER)).toBeVisible();
  await expect(result.getByText('Critères vérifiés')).toBeVisible();
  await expect(result.getByText('Critères non contrôlés')).toHaveCount(0);
  await expect(mission.getByRole('button', { name: /Arrêter/ })).toHaveCount(0);
  await noError(page);

  // Its conversation: the brief, the first stage's note, the second stage.
  await mission.getByRole('link', { name: 'Suivre dans la conversation' }).click();
  await page.waitForURL('**/chat/*');
  await expect(page.getByText('Mission confiée', { exact: true })).toBeVisible();
  await expect(page.getByText(`Mission : ${CARRIED_OUT}`)).toBeVisible();
  await expect(page.getByText('Point d’étape', { exact: true })).toBeVisible();
  await expect(page.getByText(NOTE)).toBeVisible();
  await expect(page.getByRole('note').filter({ hasText: 'Étape 2 de la mission' })).toBeVisible();
  await expect(page.getByText(ANSWER)).toBeVisible();
  await noError(page);
});

test('an answer its reviewer could not judge does not meet its criteria', async ({ page }) => {
  test.setTimeout(90_000);
  await signIn(page, demo.emma);
  await page.goto('/missions');
  const mission = await entrust(page, 'Relevé sans relecture', UNJUDGED_OBJECTIVE);
  await expect(mission.getByText('Terminée', { exact: true })).toBeVisible({ timeout: 60_000 });
  await page.reload();
  const result = mission.getByRole('region', { name: 'Résultat' });
  await expect(result.getByText(ANSWER)).toBeVisible();
  await expect(result.getByText('Critères non contrôlés (relecture indisponible)')).toBeVisible();
  await expect(result.getByText('Critères vérifiés')).toHaveCount(0);
  await noError(page);
});

test('a mission under way is stopped by its author', async ({ page }) => {
  await signIn(page, demo.emma);
  await page.goto('/missions');
  const mission = await entrust(page, STOPPED_TITLE);
  await expect(mission.getByText('En cours', { exact: true })).toBeVisible();

  // Two more under way: the page says so instead of the form.
  for (const rank of [2, 3]) {
    const entrusted = await page.request.post('/api/missions', {
      headers: WRITE,
      data: {
        title: `Relevé ${rank} (à arrêter)`,
        objective: OBJECTIVE,
        successCriteria: CRITERIA,
      } satisfies NewMission,
    });
    expect(entrusted.status()).toBe(201);
  }
  await page.reload();
  await expect(page.getByText(FULL)).toBeVisible();
  await expect(page.getByLabel('Intitulé')).toHaveCount(0);

  page.once('dialog', (dialog) => {
    expect(dialog.message()).toContain(STOPPED_TITLE);
    void dialog.accept();
  });
  await mission.getByRole('button', { name: `Arrêter « ${STOPPED_TITLE} »` }).click();
  await expect(mission.getByText('Arrêtée', { exact: true })).toBeVisible();
  // Two under way: the form is back.
  await expect(page.getByLabel('Intitulé')).toBeVisible();
  await expect(page.getByText(FULL)).toHaveCount(0);
  await page.reload();
  await expect(mission.getByText('Arrêtée', { exact: true })).toBeVisible();
  await expect(mission.getByText(STOPPED)).toBeVisible();
  await expect(mission.getByRole('button', { name: /Arrêter/ })).toHaveCount(0);
  await noError(page);
});
