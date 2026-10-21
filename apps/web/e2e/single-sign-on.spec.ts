// Signing in through the company's identity provider (ADR-063): the
// administrator declares it in « Connexion », with its client secret
// (write-only); a collaborator is offered it from their address, goes to the
// provider and comes back signed in. Disabled, it is offered to nobody; a
// failed return lands on the sign-in page with a message. The provider is
// the end-to-end server's own, an OpenID Connect provider on the private
// network signing in whoever the sign-in names (E2E_IDP_ADDRESS).
// Whatever happens, the provider is removed, and the accounts tied to it
// with it.
import type { IdentityProvidersView } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { demo, expect, noError, test } from './support/fixtures';

const PROVIDER = 'Annuaire e2e';
const ISSUER = 'http://127.0.0.1:4100';
const CLIENT_ID = 'mimorya-tests';
const CLIENT_SECRET = 'secret-client-de-test-0123456789';
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
  for (const provider of providers.filter((p) => p.name === PROVIDER)) {
    const deleted = await page.request.delete(`/api/admin/identity-providers/${provider.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

test('a collaborator signs in through the provider the company declared', async ({
  page,
  newContext,
}) => {
  await openAdmin(page, 'Connexion');
  await expect(page.getByText('http://localhost:3000/api/auth/sso/callback')).toBeVisible();
  await page.getByRole('button', { name: 'Ajouter un fournisseur' }).click();
  await page.getByLabel('Type de fournisseur').selectOption('oidc');
  await page.getByLabel('Nom affiché sur la page de connexion').fill(PROVIDER);
  await page.getByLabel('Adresse de l’émetteur').fill(ISSUER);
  await page.getByLabel('Identifiant client (ID d’application)').fill(CLIENT_ID);
  await page.getByLabel('Secret client').fill(CLIENT_SECRET);
  await page.getByLabel(/^Domaines d’e-mail/).fill('neo-maintenance.example');
  await page.getByRole('button', { name: 'Ajouter le fournisseur' }).click();

  // As the API recorded it, read again; the secret is never shown.
  await page.reload();
  await page.getByRole('tab', { name: 'Connexion' }).click();
  const card = (button: string) => cardWith(page, PROVIDER, button);
  await expect(card('Désactiver').getByText('secret enregistré')).toBeVisible();
  await expect(card('Désactiver').getByText('Domaines : neo-maintenance.example')).toBeVisible();
  expect(await page.content()).not.toContain(CLIENT_SECRET);
  await noError(page);

  // Nadia is offered it from her address, and comes back signed in.
  const nadia = await (await newContext()).newPage();
  await nadia.goto('/login');
  await nadia.getByLabel('Adresse e-mail professionnelle').fill(demo.nadia);
  await nadia.getByLabel('Adresse e-mail professionnelle').blur();
  await nadia.getByRole('button', { name: `Se connecter avec ${PROVIDER}` }).click();
  await nadia.waitForURL('**/chat');
  const me = await nadia.request.get('/api/auth/me');
  expect(((await me.json()) as { user: { email: string } }).user.email).toBe(demo.nadia);

  // Disabled, it is offered to nobody.
  await card('Désactiver').getByRole('button', { name: 'Désactiver', exact: true }).click();
  await expect(card('Activer').getByText('Désactivé', { exact: true })).toBeVisible();
  const later = await (await newContext()).newPage();
  await later.goto('/login');
  await later.getByLabel('Adresse e-mail professionnelle').fill(demo.nadia);
  const discovered = later.waitForResponse('**/api/auth/sso/discover');
  await later.getByLabel('Adresse e-mail professionnelle').blur();
  expect(await (await discovered).json()).toEqual({ provider: null });
  await expect(later.getByRole('button', { name: /^Se connecter avec/ })).toHaveCount(0);
  await noError(page);
});

test('a failed return comes back to the sign-in page, which says so', async ({ page }) => {
  // No attempt under way in this browser: refused, as a forced sign-in would be.
  await page.goto('/api/auth/sso/callback?code=inconnu&state=inconnu');
  await page.waitForURL('**/login?sso=echec');
  await expect(
    page.getByText('La connexion par votre fournisseur d’identité n’a pas abouti.', {
      exact: false,
    }),
  ).toBeVisible();
  // Still possible with the password.
  await page.getByLabel('Adresse e-mail professionnelle').fill(demo.nadia);
  await expect(page.getByRole('button', { name: 'Se connecter', exact: true })).toBeVisible();
});
