// Signing in through a SAML 2.0 provider (ADR-068): the administrator reads
// in « Connexion » the company's own identifier and the address responses
// are posted to, registers the company at the provider with the metadata
// that identifier serves, then declares the provider by pasting its own. A
// collaborator is offered it from their address, signs in at its page and
// comes back signed in. Its certificate renewed, the metadata are given
// anew: another provider's are refused, its own kept, and sign it in still.
// The provider is the end-to-end server's own (`SAML_IDP_ADDRESS` of
// `crates/api/examples/e2e_server`), whose pages carry no script: its
// response is posted back by a button. Whatever happens, the provider is
// removed, and the accounts tied to it with it.
import type { Page } from '@playwright/test';
import type { IdentityProvidersView } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { demo, expect, noError, test } from './support/fixtures';

const PROVIDER = 'Annuaire SAML e2e';
const IDP = 'http://127.0.0.1:4101';
const WRITE = { 'X-Mimorya-Request': '1' };

test.beforeEach(({ page }) => {
  // A deletion asks for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  // With the test's session; 401: the test never signed in, so declared nothing.
  const listed = await page.request.get('/api/admin/identity-providers');
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { providers } = (await listed.json()) as IdentityProvidersView;
  for (const provider of providers.filter((p) => p.name.startsWith(PROVIDER))) {
    const deleted = await page.request.delete(`/api/admin/identity-providers/${provider.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

/** Nadia, offered the provider from her address, signs in at its page. */
async function signInAtProvider(nadia: Page) {
  await nadia.goto('/login');
  await nadia.getByLabel('Adresse e-mail professionnelle').fill(demo.nadia);
  await nadia.getByLabel('Adresse e-mail professionnelle').blur();
  await nadia.getByRole('button', { name: `Se connecter avec ${PROVIDER}` }).click();
  await nadia.waitForURL((url) => url.href.startsWith(`${IDP}/sso`));
  await nadia.getByLabel('Adresse e-mail').fill(demo.nadia);
  await nadia.getByRole('button', { name: 'Se connecter' }).click();
  // Posted back to the address shown in « Connexion ».
  await nadia.getByRole('button', { name: 'Continuer' }).click();
  await nadia.waitForURL('**/chat');
  const me = await nadia.request.get('/api/auth/me');
  expect(((await me.json()) as { user: { email: string } }).user.email).toBe(demo.nadia);
}

test('the company declares a SAML provider its people sign in with', async ({
  page,
  newContext,
}) => {
  await openAdmin(page, 'Connexion');
  // The company's own identifier: the address of its metadata.
  const read = await page.request.get('/api/admin/identity-providers');
  const { samlEntityId } = (await read.json()) as IdentityProvidersView;
  expect(samlEntityId).toMatch(
    /^http:\/\/localhost:3000\/api\/auth\/saml\/[0-9a-f-]{36}\/metadata$/,
  );
  await expect(page.getByText(samlEntityId)).toBeVisible();
  await expect(page.getByText('http://localhost:3000/api/auth/saml/acs')).toBeVisible();

  // Registered at the provider as its administrator would: the metadata
  // that identifier serves, imported.
  const company = await page.request.get(samlEntityId);
  expect(company.status()).toBe(200);
  const registered = await page.request.put(`${IDP}/application`, {
    data: await company.text(),
  });
  expect(registered.status()).toBe(204);

  // Declared by the provider's metadata, pasted: no client, no secret.
  const metadata = await (await page.request.get(`${IDP}/metadata`)).text();
  await page.getByRole('button', { name: 'Ajouter un fournisseur' }).click();
  await page.getByLabel('Type de fournisseur').selectOption('saml');
  await expect(page.getByLabel('Secret client')).toHaveCount(0);
  await expect(page.getByLabel('Identifiant client (ID d’application)')).toHaveCount(0);
  await page.getByLabel('Nom affiché sur la page de connexion').fill(PROVIDER);
  await page.getByLabel('Métadonnées du fournisseur (XML)').fill(metadata);
  await page.getByLabel(/^Domaines d’e-mail/).fill('neo-maintenance.example');
  await page.getByRole('button', { name: 'Ajouter le fournisseur' }).click();
  // Saved once the form closes (the metadata are read first).
  await expect(page.getByRole('button', { name: 'Ajouter le fournisseur' })).toHaveCount(0);

  // As the API recorded it, read again: its identifier is the metadata's.
  await page.reload();
  await page.getByRole('tab', { name: 'Connexion' }).click();
  const card = (button: string) => cardWith(page, PROVIDER, button);
  await expect(card('Désactiver').getByText(`${IDP}/metadata`)).toBeVisible();
  await expect(card('Désactiver').getByText('métadonnées SAML')).toBeVisible();
  await expect(card('Désactiver').getByText('Domaines : neo-maintenance.example')).toBeVisible();
  await noError(page);

  await signInAtProvider(await (await newContext()).newPage());

  // Its certificate renewed, the metadata are given anew: another
  // provider's are refused, its own kept.
  await card('Modifier').getByRole('button', { name: 'Modifier', exact: true }).click();
  await expect(page.getByLabel(/^Nouveau secret client/)).toHaveCount(0);
  const renewed = page.getByLabel(/^Nouvelles métadonnées du fournisseur/);
  await renewed.fill(metadata.replace(`${IDP}/metadata`, 'https://autre.saml.example/metadata'));
  await page.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(page.getByText('ces métadonnées sont celles d’un autre fournisseur')).toBeVisible();
  await renewed.fill(metadata);
  await page.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Enregistrer', exact: true })).toHaveCount(0);
  await noError(page);

  // The metadata kept sign her in still.
  await signInAtProvider(await (await newContext()).newPage());
});
