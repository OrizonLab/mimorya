// Searching the company's memory by meaning, from the administration
// (ADR-074): an embedding model chosen with its key, the memory indexed in
// the background, the model tested, then back to searching by words, the
// vectors wiped. The server's embedding model is a test one: nothing leaves.
import type { EmbeddingView } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };

test.beforeEach(({ page }) => {
  // Going back to words asks for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  // 404: the company searches by words already; 401: the test never signed in.
  const reset = await page.request.delete('/api/admin/embedding', { headers: WRITE });
  expect([200, 401, 404]).toContain(reset.status());
});

test('the company searches its memory by meaning, then by words again', async ({ page }) => {
  const key = 'cle-embeddings-e2e-secrete';
  const stored = async () => {
    const shown = await page.request.get('/api/admin/embedding');
    expect(shown.status()).toBe(200);
    return ((await shown.json()) as EmbeddingView).settings;
  };
  await openAdmin(page, 'Modèle d’IA');
  await page.getByRole('button', { name: 'Chercher aussi par le sens' }).click();
  await page
    .getByRole('combobox', { name: 'Fournisseur d’embeddings' })
    .selectOption({ label: 'Mistral AI' });
  await page.getByLabel('Similarité minimale').fill('0.7');
  await page.getByLabel(/^Clé d’API d’embeddings/).fill(key);
  await page
    .getByRole('form', { name: 'Modèle d’embeddings' })
    .getByRole('button', { name: 'Enregistrer' })
    .click();
  await expect(page.getByRole('button', { name: 'Chercher aussi par le sens' })).toHaveCount(0);

  // Recorded, and the memory indexed in the background.
  await expect
    .poll(async () => {
      const settings = await stored();
      return settings !== null && settings.memories > 0 && settings.embedded === settings.memories;
    })
    .toBe(true);
  const settings = await stored();
  expect(settings).toMatchObject({
    provider: 'mistral',
    model: 'mistral-embed',
    minSimilarity: 0.7,
    keyStored: true,
    usable: true,
    indexingError: null,
  });
  const memories = settings?.memories ?? 0;
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await expect(page.getByText(/Par les mots et par le sens : Mistral AI/)).toBeVisible();
  await expect(page.getByText(`Mémoires indexées : ${memories} sur ${memories}`)).toBeVisible();
  expect(await page.content()).not.toContain(key);

  await page.getByRole('button', { name: 'Tester le modèle d’embeddings' }).click();
  await expect(page.getByText('Connexion réussie : mistral-embed a répondu.')).toBeVisible();

  await page.getByRole('button', { name: 'Chercher par les mots seulement' }).click();
  await expect(page.getByRole('button', { name: 'Chercher aussi par le sens' })).toBeVisible();
  await expect.poll(stored).toBeNull();
  await noError(page);
});
