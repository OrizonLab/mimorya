// The company's model escalation (ADR-207): on the platform's model the
// operator supplies it with (ADR-209), one is chosen beside it, told to
// count in the formula's cap or credit too; with its own provider, a
// stronger model of it is chosen within a monthly budget and a number of
// escalations per run, read back as the API recorded it, the escalation
// after a failed review and the router turned on, the router's rule for each
// kind of run chosen, shown in the consumption with its budget beside what
// each role of the model calls cost, then removed. The mechanism not delivered yet cannot be turned on.
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

test('on the platform’s model, an escalation counts in the formula too', async ({ page }) => {
  await openAdmin(page, 'Modèle d’IA');
  const form = page.getByRole('form', { name: 'Escalade de modèle' });
  const supplied = await shown(page);
  expect(supplied.onOperatorKey).toBe(true);
  await expect(form).toContainText('Votre entreprise utilise le modèle de la plateforme');
  await expect(
    form.getByLabel(`Modèle d’escalade (modèle principal : ${supplied.mainModel})`),
  ).toBeVisible();
  await form.getByRole('checkbox', { name: 'Escalade après échec du contrôle' }).check();
  await form.getByLabel(/^Modèle d’escalade/).fill(STRONGER);
  await form.getByLabel('Budget mensuel d’escalade (USD)').fill('10');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(form.getByRole('button', { name: 'Retirer l’escalade' })).toBeVisible();

  // As the API recorded it, serving.
  const chosen = await shown(page);
  expect(chosen.settings?.model).toBe(STRONGER);
  expect(chosen.settings?.monthlyBudgetMicros).toBe(10_000_000);
  expect(chosen.problem).toBeNull();
  await noError(page);
});

test('a stronger model is chosen within a monthly budget, then removed', async ({ page }) => {
  await openAdmin(page, 'Modèle d’IA');
  const form = page.getByRole('form', { name: 'Escalade de modèle' });

  // The company's own provider for its conversations.
  const own = await page.request.put('/api/admin/llm', {
    headers: WRITE,
    data: { provider: 'anthropic', model: MAIN, apiKey: 'cle-api-escalade-e2e' },
  });
  expect(own.status()).toBe(200);
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await expect(form.getByLabel(`Modèle d’escalade (modèle principal : ${MAIN})`)).toBeVisible();
  // On its own key: nothing counts in the operator's formula.
  expect((await shown(page)).onOperatorKey).toBe(false);
  await expect(form).not.toContainText('Votre entreprise utilise le modèle de la plateforme');
  // The escalation after a failed review and the router are delivered; the
  // advisor not yet.
  await expect(
    form.getByRole('checkbox', { name: 'Conseiller d’Anthropic (advisor)' }),
  ).toBeDisabled();
  await expect(form).toContainText('Bientôt disponible');
  const afterFailure = form.getByRole('checkbox', { name: 'Escalade après échec du contrôle' });
  await expect(afterFailure).toBeEnabled();
  await afterFailure.check();
  // The router's rules show once it is on.
  const router = form.getByRole('checkbox', { name: 'Routeur en amont' });
  await expect(form.getByLabel('Conversations')).toHaveCount(0);
  await router.check();
  await form.getByLabel('Conversations').selectOption('Selon un classement de la demande');
  await form.getByLabel('Déclencheurs').selectOption('Modèle d’escalade');

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
  expect([chosen.settings?.afterFailure, chosen.settings?.router]).toEqual([true, true]);
  expect(chosen.settings?.routes).toEqual({
    conversation: 'classification',
    mission: 'main',
    scheduledTask: 'main',
    hook: 'escalation',
  });
  await expect(afterFailure).toBeChecked();
  await expect(form.getByLabel('Conversations')).toHaveValue('classification');
  await expect(form.getByLabel('Missions')).toHaveValue('main');

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
