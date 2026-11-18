// The company's model escalation (ADR-207): refused while the company is on
// the platform's model; with its own provider, a stronger model of it is
// chosen within a monthly budget and a number of escalations per run, read
// back as the API recorded it, the escalation after a failed review turned
// on, shown in the consumption with its budget beside what each role of the
// model calls cost, then removed. The mechanisms not delivered yet cannot be
// turned on.
// Whatever happens, the company is back on the platform's model, without
// escalation, afterwards.
import type { Page } from '@playwright/test';
import type { ModelEscalationView } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const ESCALATION = '/api/admin/model-escalation';
const MAIN = 'claude-haiku-5-5';
const STRONGER = 'claude-opus-5-5';

test.beforeEach(({ page }) => {
  // Removing the escalation asks for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  // 404: nothing to remove; 401: the test never signed in, so changed nothing.
  for (const path of [ESCALATION, '/api/admin/llm']) {
    const reset = await page.request.delete(path, { headers: WRITE });
    expect([200, 401, 404]).toContain(reset.status());
  }
});

async function shown(page: Page): Promise<ModelEscalationView> {
  return (await (await page.request.get(ESCALATION)).json()) as ModelEscalationView;
}

test('a stronger model is chosen within a monthly budget, then removed', async ({ page }) => {
  await openAdmin(page, 'Modèle d’IA');
  const form = page.getByRole('form', { name: 'Escalade de modèle' });
  await expect(form).toContainText('L’escalade demande le fournisseur propre');
  await expect(form.getByRole('button', { name: 'Enregistrer' })).toBeDisabled();

  // The company's own provider for its conversations.
  const own = await page.request.put('/api/admin/llm', {
    headers: WRITE,
    data: { provider: 'anthropic', model: MAIN, apiKey: 'cle-api-escalade-e2e' },
  });
  expect(own.status()).toBe(200);
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await expect(form.getByLabel(`Modèle d’escalade (modèle principal : ${MAIN})`)).toBeVisible();
  // The escalation after a failed review is delivered; the others not yet.
  for (const label of ['Routeur en amont', 'Conseiller d’Anthropic (advisor)']) {
    await expect(form.getByRole('checkbox', { name: label })).toBeDisabled();
  }
  await expect(form).toContainText('Bientôt disponible');
  const afterFailure = form.getByRole('checkbox', { name: 'Escalade après échec du contrôle' });
  await expect(afterFailure).toBeEnabled();
  await afterFailure.check();

  await form.getByLabel(/^Modèle d’escalade/).fill(STRONGER);
  await form.getByLabel('Budget mensuel d’escalade (USD)').fill('25');
  await form.getByLabel('Escalades au plus par run').fill('3');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(form.getByRole('button', { name: 'Retirer l’escalade' })).toBeVisible();

  // As the API recorded it, after a reload.
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await expect(form.getByLabel(/^Modèle d’escalade/)).toHaveValue(STRONGER);
  const chosen = await shown(page);
  expect(chosen.settings?.model).toBe(STRONGER);
  expect(chosen.settings?.monthlyBudgetMicros).toBe(25_000_000);
  expect(chosen.settings?.maxPerRun).toBe(3);
  expect([chosen.settings?.afterFailure, chosen.settings?.router]).toEqual([true, false]);
  await expect(afterFailure).toBeChecked();

  // The consumption shows the escalation's budget for the month, and what
  // each role cost: the server left a call of each at its start
  // (`e2e_server/usage_records.rs`); the scripted model's cost nothing.
  await page.getByRole('tab', { name: 'Consommation' }).click();
  await expect(page.getByText(/Escalade de modèle depuis le/)).toBeVisible();
  await expect(page.getByText(/sur 25,000\s\$US/)).toBeVisible();
  const roles = page.getByRole('list', { name: 'Répartition par rôle' });
  await expect(roles).toContainText(/Modèle principal : 2,500\s\$US/);
  await expect(roles).toContainText(/Escalade : 1,000\s\$US/);
  await expect(roles).toContainText(/Contrôle des réponses : 0,500\s\$US/);
  const escalated = page.getByRole('row').filter({ hasText: 'claude-opus-5-5' });
  await expect(escalated.getByRole('cell').nth(1)).toHaveText('Escalade');

  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await form.getByRole('button', { name: 'Retirer l’escalade' }).click();
  await expect(form.getByRole('button', { name: 'Retirer l’escalade' })).toHaveCount(0);
  expect((await shown(page)).settings).toBeNull();
  await noError(page);
});
