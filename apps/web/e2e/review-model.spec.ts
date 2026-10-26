// The model that reviews the assistant's answers, chosen apart from the one of
// the conversations (ADR-108): chosen with its own write-only key, tested,
// read back as the API recorded it, then removed, the conversations' model
// reviewing again. Whatever happens, the company has no review model of its
// own afterwards.
import type { Page } from '@playwright/test';
import type { LlmView } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const REVIEW = '/api/admin/llm/verification';
const MODEL = 'claude-haiku-5-5';
const BY_THE_CONVERSATIONS = 'Les réponses sont relues par le modèle de vos conversations.';

test.beforeEach(({ page }) => {
  // The return to the conversations' model asks for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  // 404: none was chosen; 401: the test never signed in, so changed nothing.
  const reset = await page.request.delete(REVIEW, { headers: WRITE });
  expect([200, 401, 404]).toContain(reset.status());
});

async function shown(page: Page): Promise<LlmView> {
  return (await (await page.request.get('/api/admin/llm')).json()) as LlmView;
}

test('a model is chosen to review the answers, tested, then removed', async ({ page }) => {
  const key = 'cle-api-controle-e2e';
  await openAdmin(page, 'Modèle d’IA');
  const section = page.getByRole('region', { name: 'Modèle de contrôle des réponses' });
  await expect(section).toContainText(BY_THE_CONVERSATIONS);

  await section.getByRole('button', { name: 'Choisir un modèle de contrôle' }).click();
  const form = section.getByRole('form', { name: 'Modèle de contrôle' });
  await form
    .getByRole('combobox', { name: 'Fournisseur' })
    .selectOption({ label: 'Anthropic (Claude)' });
  await form.getByLabel('Modèle', { exact: true }).fill(MODEL);
  await form.getByLabel(/^Clé d’API du modèle de contrôle/).fill(key);
  await form.getByRole('button', { name: 'Enregistrer' }).click();

  await expect(section).toContainText(`Modèle de contrôle : Anthropic (Claude) · ${MODEL}`);
  await expect(section.getByText('Clé enregistrée')).toBeVisible();
  expect(await page.content()).not.toContain(key);
  // The conversations keep the platform's model.
  await expect(page.getByText(/Fournisseur de votre entreprise/)).toHaveCount(0);
  await section.getByRole('button', { name: 'Tester le modèle de contrôle' }).click();
  await expect(section.getByText(/Connexion réussie/)).toBeVisible();

  // As the API recorded it, after a reload.
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await expect(section).toContainText(`Modèle de contrôle : Anthropic (Claude) · ${MODEL}`);
  const chosen = await shown(page);
  expect(chosen.settings).toBeNull();
  expect([chosen.verification?.provider, chosen.verification?.model]).toEqual(['anthropic', MODEL]);
  expect(chosen.verification?.keyStored).toBe(true);

  await section.getByRole('button', { name: 'Revenir au modèle des conversations' }).click();
  await expect(section).toContainText(BY_THE_CONVERSATIONS);
  expect((await shown(page)).verification).toBeNull();
  await noError(page);
});
