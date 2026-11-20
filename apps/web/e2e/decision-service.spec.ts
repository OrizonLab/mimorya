// The company's decision service (ADR-213): Paul finds its main model chosen
// and every usage off, the model's routing alone delivered (D1); chooses
// TypeSafe's Jev, which the operator opens here, warned that it is a
// subprocessor outside the European Union, with the company's key, and sets
// the model's routing active at a threshold; reads it back once the page is
// reloaded, the key kept and never shown; tests it against the local
// TypeSafe (`e2e_server`, `FakeJev`); and goes back to the main model, the
// key removed. Whatever happens, the settings are removed afterwards. The
// rules themselves are tested on the API (`crates/api/tests/decision_*.rs`).
import type { Page } from '@playwright/test';
import type { DecisionSettingsView } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const DECISIONS = '/api/admin/decisions';
/** The only key the local TypeSafe opens to (`JEV_KEY`). */
const JEV_KEY = 'ts-cle-simulee-jev-0123456789abcdef';

test.beforeEach(({ page }) => {
  // Going back to the defaults asks for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  // 404: nothing to remove; 401: the test never signed in, so changed nothing.
  const reset = await page.request.delete(DECISIONS, { headers: WRITE });
  expect([200, 401, 404]).toContain(reset.status());
});

async function shown(page: Page): Promise<DecisionSettingsView> {
  return (await (await page.request.get(DECISIONS)).json()) as DecisionSettingsView;
}

async function openTheTab(page: Page) {
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
}

test('a company chooses Jev for its decisions, then its main model again', async ({ page }) => {
  await openAdmin(page, 'Modèle d’IA');
  const form = page.getByRole('form', { name: 'Service de décision' });
  const main = form.getByRole('radio', { name: /^Modèle principal/ });
  const jev = form.getByRole('radio', { name: /^Jev de TypeSafe \(jev-1\.14\.0\)/ });
  await expect(main).toBeChecked();
  // The main model's probabilities are said not to be calibrated.
  const uncalibrated = form.getByText(/cette probabilité n’est pas calibrée/);
  await expect(uncalibrated).toBeVisible();
  // The model's routing, the skill choice and the memory guard are
  // delivered (D1 to D3), off until set; the others wait for their step.
  const routing = form.getByLabel('Mode : Choix du modèle d’un run');
  await expect(routing).toBeEnabled();
  await expect(routing).toHaveValue('off');
  await expect(form.getByLabel('Mode : Choix des compétences à charger')).toBeEnabled();
  await expect(
    form.getByLabel('Mode : Données sensibles dans la mémoire et les leçons'),
  ).toBeEnabled();
  await expect(
    form.getByLabel('Mode : Type et confidentialité des documents déposés'),
  ).toBeDisabled();
  await expect(form.getByText('(bientôt disponible)')).toHaveCount(4);

  await jev.check();
  await expect(form).toContainText('sous-traitant situé hors de l’Union européenne');
  // Said too where Jev is chosen: the main model decides when it does not answer.
  await expect(uncalibrated).toBeVisible();
  await routing.selectOption('active');
  await form.getByLabel('Seuil : Choix du modèle d’un run').fill('0.7');
  await form.getByLabel(/^Clé d’API TypeSafe/).fill(JEV_KEY);
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(form.getByLabel(/laisser vide pour conserver la clé enregistrée/)).toBeVisible();

  // Read back from the API once the page is reloaded, the key never shown.
  await openTheTab(page);
  await expect(jev).toBeChecked();
  await expect(form.getByLabel(/^Clé d’API TypeSafe/)).toHaveValue('');
  const response = await page.request.get(DECISIONS);
  expect(await response.text()).not.toContain(JEV_KEY);
  const chosen = (await response.json()) as DecisionSettingsView;
  expect([chosen.engine, chosen.hasJevKey, chosen.problem]).toEqual(['jev', true, null]);
  const routed = chosen.usages.find((usage) => usage.key === 'modelRouting');
  expect([routed?.mode, routed?.threshold]).toEqual(['active', 0.7]);
  await expect(routing).toHaveValue('active');

  // The engine chosen answers the test's question.
  await form.getByRole('button', { name: 'Tester le moteur' }).click();
  await expect(form).toContainText(
    /jev-1\.14\.0 a répondu en \d+ ms : le message d’essai est une salutation avec une probabilité de 97 %\./,
  );
  await noError(page);

  // The consumption shows Jev's dated price, and its call with the role « Décision ».
  await page.getByRole('tab', { name: 'Consommation' }).click();
  const price = page.getByRole('list', { name: 'Tarif de Jev' });
  await expect(price).toContainText(
    'jev-1.14.0 : 0,042 $US par million de tokens en entrée, sortie gratuite',
  );
  await expect(
    page.getByText(/^Tarif de Jev \(TypeSafe\), relevé le \d\d\/\d\d\/\d{4}$/),
  ).toBeVisible();
  await expect(page.getByRole('row', { name: /jev-1\.14\.0 \(typesafe\) Décision/ })).toBeVisible();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();

  // Back to the main model, the key removed.
  await form.getByRole('button', { name: 'Revenir aux réglages par défaut' }).click();
  await expect(main).toBeChecked();
  await expect(form.getByRole('button', { name: 'Revenir aux réglages par défaut' })).toHaveCount(
    0,
  );
  const back = await shown(page);
  expect([back.engine, back.hasJevKey, back.updatedAt]).toEqual(['mainModel', false, null]);
  await noError(page);
});
