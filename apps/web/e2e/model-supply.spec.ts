// The company's own model provider as a company the operator does not
// supply sees it (ADR-208): removing it leaves the company without a model,
// which the screen and its confirmation say, rather than a return to the
// platform's. The demo company is supplied, and the browser cannot change
// that (only the operator's command does): the screen is shown the API's
// view of the model without the platform's, as the API answers a company
// it does not supply. A supplied company still goes back to the platform's.
// The model tab shows the company's formula and what its month cost under
// its cap, as the API reads them (ADR-210); and a prepaid credit's balance
// and payments, the screen shown the API's view of such a company, whose
// formula only the operator's command sets.
// Whatever happens, the company has no provider of its own afterwards.
import type { Page } from '@playwright/test';
import type { LlmView, ModelSupplyView } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const LLM = '/api/admin/llm';
const SUPPLY = '/api/admin/model-supply';
const SPENT =
  'Le crédit de consommation du modèle de votre entreprise est épuisé : contactez votre administrateur.';
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

/** A cost as the screens write it (`1 234,500 $US`), whatever spaces the
 * browser puts between its groups of digits. */
function cost(micros: number): RegExp {
  const written = (micros / 1_000_000).toLocaleString('fr-FR', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 3,
  });
  const escaped = written.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(escaped.replace(/\s/g, '\\s'));
}

test('the formula and what the month cost under its cap are read in the model tab', async ({
  page,
}) => {
  await openAdmin(page, 'Modèle d’IA');
  const view = (await (await page.request.get(SUPPLY)).json()) as ModelSupplyView;
  expect(view.formula).toBe('capped');
  const supply = page.getByRole('region', { name: 'Fourniture du modèle' });
  await expect(supply).toContainText('Modèle de la plateforme, avec un plafond mensuel');
  const month = supply.getByText(/Consommé depuis le/);
  await expect(month).toContainText(cost(view.monthBilledMicros));
  await expect(month).toContainText(cost(view.monthlyCapMicros ?? 0));
  await noError(page);
});

test('a prepaid credit shows its balance and its payments, the latest first', async ({ page }) => {
  await openAdmin(page, 'Modèle d’IA');
  await page.route(`**${SUPPLY}`, async (route) => {
    const response = await route.fetch();
    const view = (await response.json()) as ModelSupplyView;
    const prepaid = {
      ...view,
      formula: 'prepaid',
      monthlyCapMicros: null,
      balanceMicros: 0,
      payments: [
        { amountMicros: 50_000_000, reference: 'FAC-2026-0412', recordedAt: view.monthStart },
        { amountMicros: 20_000_000, reference: 'FAC-2026-0388', recordedAt: view.monthStart },
      ],
      stopped: SPENT,
    } satisfies ModelSupplyView;
    await route.fulfill({ response, json: prepaid });
  });
  await modelTab(page);
  const supply = page.getByRole('region', { name: 'Fourniture du modèle' });
  await expect(supply).toContainText('Modèle de la plateforme, sur un crédit prépayé');
  await expect(supply.getByText('Solde du crédit')).toContainText(cost(0));
  await expect(supply).toContainText(SPENT);
  const rows = supply.getByRole('row');
  await expect(rows.nth(1)).toContainText('FAC-2026-0412');
  await expect(rows.nth(1)).toContainText(cost(50_000_000));
  await expect(rows.nth(2)).toContainText('FAC-2026-0388');
  await noError(page);
});
