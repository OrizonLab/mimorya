// The reference scenario through the browser (ROADMAP, A13): a manager asks
// the assistant, follows the run live, validates the planning it proposes,
// reads the result, then finds the run and the decision in the operations.
import { demo, expect, signIn, test } from './support/fixtures';

const REQUEST =
  'Fais le point sur les interventions de la semaine et planifie le suivi de celles qui ne sont pas terminées.';

test('a manager asks, validates the planning and gets the result', async ({ page }) => {
  await signIn(page, demo.marc);
  await page.getByLabel('Votre message').fill(REQUEST);
  await page.getByRole('button', { name: 'Envoyer' }).click();

  // The run pauses on the action that changes the planning.
  await expect(page.getByText('Validation requise')).toBeVisible({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Valider' }).click();
  await expect(page.getByRole('heading', { name: 'Synthèse des interventions' })).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByText('suivi programmé lundi prochain à 13 h avec Léa')).toBeVisible();

  // The operations show the run, completed, and what it called.
  await page.goto('/operations');
  const run = page.getByRole('link').filter({ hasText: 'Fais le point sur les interventions' });
  await expect(run.first()).toContainText('Terminée');
  await run.first().click();
  await expect(page.getByText('planning_apply').first()).toBeVisible();
});
