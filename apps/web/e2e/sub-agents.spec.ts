// Sub-agents (ADR-098, ADR-099): Lucas asks a question the agent hands to a
// research sub-agent, which searches the documents; the answer comes from
// its summary and cites the charter it read. In « Opérations », the request
// is listed alone, its detail names its sub-agent, whose own detail shows
// what it read and leads back to the request.
import { demo, expect, noError, signIn, test } from './support/fixtures';

/** The question, the task and the answer the server's model plays (`e2e_server/sub_agents.rs`). */
const ASKED =
  'Fais rechercher par un sous-agent le délai de prise en charge d’une panne de chauffage.';
const TASK = 'Trouver dans les documents le délai de prise en charge d’une panne de chauffage';
const ANSWERED = /D’après la recherche confiée à un sous-agent/;

test('a question handed to a sub-agent is traced with its sub-agent', async ({ page }) => {
  await signIn(page, demo.lucas);
  await page.getByLabel('Votre message').fill(ASKED);
  await page.getByRole('button', { name: 'Envoyer' }).click();
  await expect(page.getByText(ANSWERED)).toBeVisible({ timeout: 30_000 });
  // What the sub-agent read is the answer's source.
  await page.getByText(/source\(s\) consultée\(s\)/).click();
  await expect(
    page.getByRole('link', {
      name: 'Télécharger le document « Charte qualité des interventions »',
    }),
  ).toBeVisible();

  // Listed alone: its sub-agent goes with it.
  await page.getByRole('link', { name: 'Opérations' }).click();
  const request = page.getByRole('link', { name: new RegExp(ASKED) }).first();
  await expect(request).toBeVisible();
  await expect(page.getByText(TASK)).toHaveCount(0);
  await request.click();

  await expect(page.getByRole('heading', { name: 'Détail de l’opération' })).toBeVisible();
  await expect(page.getByRole('cell', { name: 'delegate_task' })).toBeVisible();
  const subAgents = page.getByRole('region', { name: 'Sous-agents' });
  const child = subAgents.getByRole('link', { name: new RegExp(TASK) });
  await expect(child).toContainText('Recherche');
  await expect(child).toContainText('Terminée');
  await child.click();

  // The sub-agent: what it read, and the request it worked for.
  await expect(page.getByRole('heading', { name: 'Sous-agent « Recherche »' })).toBeVisible();
  await expect(page.getByText(TASK)).toBeVisible();
  await expect(page.getByRole('cell', { name: 'document_search' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Ouvrir la conversation' })).toHaveCount(0);
  await page.getByRole('link', { name: 'Demande d’origine' }).click();
  await expect(page.getByRole('heading', { name: 'Détail de l’opération' })).toBeVisible();
  await expect(page.getByText(ASKED)).toBeVisible();
  await noError(page);
});
