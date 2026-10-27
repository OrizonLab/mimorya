// The company's Microsoft 365 application and the accounts its people link
// there (ADR-116, ADR-118): Paul declares the application (its secret never
// listed) and is shown the return address to register in it; Léa, from
// « Mon compte », links her account at the test provider, which plays
// Microsoft and comes back to her page, then removes the link; Paul sees
// how many people linked theirs, renames the application and deletes it.
// Each step is read back from the API. The application issuer is the test
// provider's (the end-to-end server replaces Microsoft's): its connector is
// never started, its tools never called. Whatever happens, the application
// is deleted, with its links.
import type { Page } from '@playwright/test';
import type { AccountLinks, ConnectorView, Connectors } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const NAME = 'Microsoft 365 e2e';
const RENAMED = 'Messagerie e2e';
const DIRECTORY = '72f988bf-86f1-41af-91ab-2d7cd011db47';
/** The test provider's application, and its secret. */
const APPLICATION = '0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0';
const SECRET = 'secret-client-de-test-0123456789';
const WRITE = { 'X-Mimorya-Request': '1' };

test.beforeEach(({ page }) => {
  // Deletions ask for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/admin/connectors');
  // 401: the test never signed in; 403: not as an administrator. Either
  // way, it declared nothing.
  if (listed.status() === 401 || listed.status() === 403) return;
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.type === 'microsoft365')) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

/** The company's application as the API records it, and the whole listing. */
async function stored(page: Page): Promise<{ app?: ConnectorView; listing: string }> {
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { instances } = JSON.parse(listing) as Connectors;
  return { app: instances.find((c) => c.type === 'microsoft365'), listing };
}

/** The person's link at the application, as the API lists it to them. */
async function linkOf(page: Page) {
  const listed = await page.request.get('/api/account/links');
  expect(listed.status()).toBe(200);
  const { connectors } = (await listed.json()) as AccountLinks;
  return connectors.find((c) => c.type === 'microsoft365');
}

test('an application is declared, an account linked and removed, the application deleted', async ({
  page,
  newContext,
}) => {
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Déclarer l’application Microsoft 365' }).click();
  const form = page.getByRole('form', { name: 'Application Microsoft 365' });
  await form.getByLabel('Nom', { exact: true }).fill(NAME);
  await form.getByLabel('ID de l’annuaire (locataire)').fill(DIRECTORY);
  await form.getByLabel('ID de l’application (client)').fill(APPLICATION);
  await form.getByLabel('Secret de l’application').fill(SECRET);
  await form.getByRole('button', { name: 'Déclarer l’application' }).click();
  const card = cardWith(page, NAME, 'Supprimer');
  await expect(card.getByText('0 compte(s) relié(s)')).toBeVisible();
  await expect(card.getByText('/api/account/links/callback')).toBeVisible();
  // One per company: no second declaration is offered.
  await expect(
    page.getByRole('button', { name: 'Déclarer l’application Microsoft 365' }),
  ).toHaveCount(0);
  await noError(page);
  const { app, listing } = await stored(page);
  expect(app?.name).toBe(NAME);
  expect(app?.microsoft365?.directory).toBe(DIRECTORY);
  expect(app?.microsoft365?.clientId).toBe(APPLICATION);
  expect(app?.microsoft365?.secretStored).toBe(true);
  expect(listing).not.toContain(SECRET);

  // Léa links her own account, at the provider and back to her page.
  const lea = await (await newContext()).newPage();
  await signIn(lea, demo.lea);
  await lea.goto('/account');
  const section = lea.getByRole('region', { name: NAME });
  await expect(section.getByText('Non relié')).toBeVisible();
  await section.getByRole('button', { name: 'Relier mon compte' }).click();
  await lea.waitForURL('**/account');
  await expect(lea.getByRole('status')).toContainText('Votre compte Microsoft 365 est relié');
  await expect(section.getByText(`Relié à ${demo.lea}`)).toBeVisible();
  await noError(lea);
  expect((await linkOf(lea))?.linked?.address).toBe(demo.lea);
  // Said once: the page reloaded no longer says it.
  await lea.reload();
  await expect(section.getByText(`Relié à ${demo.lea}`)).toBeVisible();
  await expect(lea.getByText('Votre compte Microsoft 365 est relié')).toHaveCount(0);

  // Paul sees it counted, and renames the application.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await expect(card.getByText('1 compte(s) relié(s)')).toBeVisible();
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const edit = page.getByRole('form', { name: 'Application Microsoft 365' });
  await edit.getByLabel('Nom', { exact: true }).fill(RENAMED);
  await edit.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(cardWith(page, RENAMED, 'Supprimer')).toBeVisible();
  expect((await stored(page)).app?.name).toBe(RENAMED);

  // Léa removes her link.
  lea.on('dialog', (dialog) => dialog.accept());
  await lea.reload();
  const renamed = lea.getByRole('region', { name: RENAMED });
  await renamed.getByRole('button', { name: 'Retirer la liaison' }).click();
  await expect(renamed.getByText('Non relié')).toBeVisible();
  expect((await linkOf(lea))?.linked).toBeNull();

  // Paul deletes the application.
  await cardWith(page, RENAMED, 'Supprimer').getByRole('button', { name: 'Supprimer' }).click();
  await expect(page.getByText(RENAMED)).toHaveCount(0);
  expect((await stored(page)).app).toBeUndefined();
});

test('a failed return is said on « Mon compte », once', async ({ page }) => {
  await signIn(page, demo.lea);
  await page.goto('/account?liaison=echec');
  await expect(page.getByText('La liaison a échoué')).toBeVisible();
  await expect(page).toHaveURL(/\/account$/);
  await page.reload();
  await expect(page.getByText('La liaison a échoué')).toHaveCount(0);
});
