// The company's own model provider as a company the operator does not
// supply sees it (ADR-208): removing it leaves the company without a model,
// which the screen and its confirmation say, rather than a return to the
// platform's. The demo company is supplied, and the browser cannot change
// that (only the operator's command does): the screen is shown the API's
// view of the model without the platform's, as the API answers a company
// it does not supply. A supplied company still goes back to the platform's.
// Whatever happens, the company has no provider of its own afterwards.
import type { Page } from '@playwright/test';
import type { LlmView } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const LLM = '/api/admin/llm';
const OWN = { provider: 'anthropic', model: 'claude-haiku-5-5', apiKey: 'cle-api-fourniture-e2e' };

test.afterEach(async ({ page }) => {
  // 404: none was left; 401: the test never signed in, so changed nothing.
  const reset = await page.request.delete(LLM, { headers: WRITE });
  expect([200, 401, 404]).toContain(reset.status());
});

/** The model screen of a company the operator does not supply. */
async function unsupplied(page: Page) {
  await page.route(`**${LLM}`, async (route) => {
    if (route.request().method() !== 'GET') return route.continue();
    const response = await route.fetch();
    const view = (await response.json()) as LlmView;
    await route.fulfill({ response, json: { ...view, platform: null } satisfies LlmView });
  });
}

async function modelTab(page: Page) {
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
}

test('a company the operator does not supply removes its own provider knowing what follows', async ({
  page,
}) => {
  await openAdmin(page, 'Modèle d’IA');
  const own = await page.request.put(LLM, { headers: WRITE, data: OWN });
  expect(own.status()).toBe(200);
  await modelTab(page);
  // Supplied, the company goes back to the platform's model.
  await expect(
    page.getByRole('button', { name: 'Revenir au modèle de la plateforme' }),
  ).toBeVisible();

  await unsupplied(page);
  await modelTab(page);
  const remove = page.getByRole('button', { name: 'Retirer ce fournisseur' });
  await expect(remove).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Revenir au modèle de la plateforme' }),
  ).toHaveCount(0);
  // The confirmation says what follows; declined, nothing is removed.
  page.once('dialog', (dialog) => {
    expect(dialog.message()).toContain('vos conversations n’auront plus de modèle');
    void dialog.dismiss();
  });
  await remove.click();
  const kept = (await (await page.request.get(LLM)).json()) as LlmView;
  expect(kept.settings?.model).toBe(OWN.model);

  page.once('dialog', (dialog) => void dialog.accept());
  await remove.click();
  await expect(
    page.getByText(
      'La plateforme ne fournit pas de modèle à votre entreprise : configurez le vôtre.',
    ),
  ).toBeVisible();
  // As the API recorded it.
  const removed = (await (await page.request.get(LLM)).json()) as LlmView;
  expect(removed.settings).toBeNull();
  await noError(page);
});
