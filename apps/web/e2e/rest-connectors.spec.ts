// A REST API the company describes itself (ADR-112, ADR-113): Paul adds it
// with its key and two operations, is told why a path that would climb is
// refused, checks the declaration (the API is not called), maps a read,
// changes the API (an operation retitled, another removed, then the read
// made a deletion, whose mapping goes, the former rules never left to be
// saved again) and deletes it, each step read back from the API. The key is never listed. The address is
// never reached: a check only reads the declaration. Whatever happens, the
// test's API is deleted.
import type { Page } from '@playwright/test';
import type { ConnectorView, Connectors } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, FOREIGN_HOST, noError, test } from './support/fixtures';

const API = 'CRM e2e';
const KEY = 'cle-rest-e2e-0123456789';
const URL = `https://api.${FOREIGN_HOST}/v1`;
const WRITE = { 'X-Mimorya-Request': '1' };
const READ = 'rest_crme_get_ticket';

test.beforeEach(({ page }) => {
  // Deletions ask for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/admin/connectors');
  // 401: the test never signed in, so changed nothing.
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.name === API)) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

/** The test's API as the API records it, and the whole listing. */
async function stored(page: Page): Promise<{ api?: ConnectorView; listing: string }> {
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { instances } = JSON.parse(listing) as Connectors;
  return { api: instances.find((c) => c.name === API), listing };
}

test('a REST API is described, checked, mapped, changed and deleted', async ({ page }) => {
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Ajouter une API REST' }).click();
  const form = page.locator('form');
  await form.getByLabel('Nom', { exact: true }).fill(API);
  await form.getByLabel('Préfixe des outils (définitif)').fill('crme');
  await form.getByLabel(/^Adresse de l’API/).fill(URL);
  await form.getByLabel(/^En-tête du jeton/).fill('X-Api-Key');
  await form.getByLabel(/^Jeton d’accès/).fill(KEY);

  const first = form.getByRole('group', { name: 'Opération 1' });
  await first.getByLabel('Nom de l’opération').fill('get_ticket');
  await first.getByLabel('Intitulé').fill('Lire un ticket');
  // A path that would climb out of what is written: refused, and said.
  await first.getByLabel(/^Chemin/).fill('/tickets/../{id}');
  await first.getByLabel('Description lue par le modèle').fill('Renvoie un ticket et son état.');
  await first.getByRole('button', { name: 'Ajouter un paramètre' }).click();
  const id = first.getByRole('group', { name: 'Paramètre 1' });
  await id.getByLabel('Nom', { exact: true }).fill('id');
  await id.getByLabel('Emplacement').selectOption('path');
  await expect(id.getByLabel('Requis')).toBeChecked();

  await form.getByRole('button', { name: 'Ajouter une opération' }).click();
  const second = form.getByRole('group', { name: 'Opération 2' });
  await second.getByLabel('Nom de l’opération').fill('create_ticket');
  await second.getByLabel('Intitulé').fill('Créer un ticket');
  await second.getByLabel('Méthode').selectOption('POST');
  await second.getByLabel(/^Chemin/).fill('/tickets');
  await second.getByLabel('Description lue par le modèle').fill('Crée un ticket.');
  await second.getByRole('button', { name: 'Ajouter un paramètre' }).click();
  const title = second.getByRole('group', { name: 'Paramètre 1' });
  await title.getByLabel('Nom', { exact: true }).fill('title');
  await title.getByLabel('Emplacement').selectOption('body');
  await title.getByLabel('Requis').check();

  await form.getByRole('button', { name: 'Ajouter l’API' }).click();
  await expect(
    form.getByText('Opération get_ticket : segment . ou .. refusé dans le chemin'),
  ).toBeVisible();
  expect((await stored(page)).api).toBeUndefined();
  await first.getByLabel(/^Chemin/).fill('/tickets/{id}');
  await form.getByRole('button', { name: 'Ajouter l’API' }).click();
  await expect(page.locator('form')).toHaveCount(0);

  const added = await stored(page);
  expect(added.api?.type).toBe('rest');
  expect(added.api?.rest).toMatchObject({
    prefix: 'crme',
    url: URL,
    tokenHeader: 'X-Api-Key',
    tokenStored: true,
  });
  expect(added.api?.rest?.operations.map((o) => o.name)).toEqual(['get_ticket', 'create_ticket']);
  expect(added.api?.rest?.operations[0]?.parameters[0]).toMatchObject({
    name: 'id',
    in: 'path',
    required: true,
  });
  expect(added.api?.rest?.operations[1]?.parameters[0]).toMatchObject({
    name: 'title',
    in: 'body',
    type: 'string',
    required: true,
  });
  expect(added.listing).not.toContain(KEY);

  const card = cardWith(page, API, 'Vérifier la déclaration');
  await expect(card.getByText('rest_crme_…')).toBeVisible();
  await expect(card.getByText(/2 opération\(s\) · jeton envoyé en « X-Api-Key »/)).toBeVisible();
  await expect(card.getByText('Non vérifiée', { exact: true })).toBeVisible();
  await card.getByRole('button', { name: 'Vérifier la déclaration' }).click();
  await expect(
    card.getByText(/^Déclaration vérifiée : 2 outil\(s\)\. L’API n’est pas appelée/),
  ).toBeVisible();
  // An operation's description is written in its declaration.
  await expect(card.getByLabel(`Description de ${READ} lue par le modèle`)).toHaveCount(0);
  await card.getByLabel(`Risque de ${READ}`).selectOption('read');
  await card.getByRole('button', { name: 'Enregistrer les règles des outils' }).click();
  await expect.poll(async () => (await stored(page)).api?.toolPolicies[READ]?.risk).toBe('read');
  await expect(card.getByText('Déclaration vérifiée', { exact: true })).toBeVisible();

  // Changed: the first retitled, the second removed; the prefix stays.
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const edit = page.locator('form');
  await expect(edit.getByLabel('Préfixe des outils (définitif)')).toBeDisabled();
  await edit
    .getByRole('group', { name: 'Opération 1' })
    .getByLabel('Intitulé')
    .fill('Lire un ticket client');
  await edit.getByRole('button', { name: 'Retirer l’opération 2' }).click();
  await edit.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(page.locator('form')).toHaveCount(0);
  const changed = (await stored(page)).api;
  expect(changed?.rest?.operations).toHaveLength(1);
  expect(changed?.rest?.operations[0]).toMatchObject({
    name: 'get_ticket',
    title: 'Lire un ticket client',
  });
  // The key and the read's mapping stay: same address, same request.
  expect(changed?.rest?.tokenStored).toBe(true);
  expect(changed?.toolPolicies[READ]?.risk).toBe('read');

  // Rules shown by a check, then the read changed into a deletion: the
  // former rules are not left to be saved again, and its mapping is gone.
  const rules = card.getByRole('button', { name: 'Enregistrer les règles des outils' });
  await card.getByRole('button', { name: 'Vérifier la déclaration' }).click();
  await expect(rules).toBeVisible();
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  await expect(rules).toHaveCount(0);
  await page.locator('form').getByLabel('Méthode').selectOption('DELETE');
  await page.locator('form').getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(page.locator('form')).toHaveCount(0);
  await expect(rules).toHaveCount(0);
  await expect.poll(async () => (await stored(page)).api?.toolPolicies).toEqual({});
  expect((await stored(page)).api?.rest?.operations[0]?.method).toBe('DELETE');

  await card.getByRole('button', { name: 'Supprimer', exact: true }).click();
  await expect.poll(async () => (await stored(page)).api).toBeUndefined();
  await expect(page.getByText('rest_crme_…')).toHaveCount(0);
  await noError(page);
});
