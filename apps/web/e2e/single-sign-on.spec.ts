// Signing in through the company's identity provider (ADR-063): the
// administrator declares it in « Connexion », with its client secret
// (write-only), then renames it and changes its domains without retyping
// the secret, which is kept; a collaborator is offered it from their
// address, goes to the provider and comes back signed in. Disabled, it is
// offered to nobody; deleted, it is gone. An invited collaborator activates
// the account through it, from the invitation's page, without choosing a
// password; « Mon compte » then says it has none (ADR-064). A failed return
// lands on the sign-in page with a message, and so does a discovery refused
// past the limit of addresses asked about. The provider is the end-to-end
// server's own, an OpenID Connect provider on the private network signing
// in whoever the sign-in names (`IDP_ADDRESS` of
// `crates/api/examples/e2e_server`).
// Whatever happens, the provider is removed, and the accounts tied to it
// with it.
import type {
  Directory,
  IdentityProvidersView,
  NewAccount,
  NewIdentityProvider,
} from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { lastLink } from './support/mail';

const PROVIDER = 'Annuaire e2e';
const RENAMED = `${PROVIDER} renommé`;
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
  for (const provider of providers.filter((p) => p.name.startsWith(PROVIDER))) {
    const deleted = await page.request.delete(`/api/admin/identity-providers/${provider.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

test('the company declares, changes and removes the provider its people sign in with', async ({
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
  // Saved once the form closes (the provider is reached first).
  await expect(page.getByRole('button', { name: 'Ajouter le fournisseur' })).toHaveCount(0);

  // As the API recorded it, read again; the secret is never shown.
  await page.reload();
  await page.getByRole('tab', { name: 'Connexion' }).click();
  const card = (button: string) => cardWith(page, PROVIDER, button);
  await expect(card('Désactiver').getByText('secret enregistré')).toBeVisible();
  await expect(card('Désactiver').getByText('Domaines : neo-maintenance.example')).toBeVisible();
  expect(await page.content()).not.toContain(CLIENT_SECRET);
  await noError(page);

  // Renamed, another domain added, the secret left blank: kept.
  await card('Modifier').getByRole('button', { name: 'Modifier', exact: true }).click();
  await page.getByLabel('Nom affiché sur la page de connexion').fill(RENAMED);
  await page.getByLabel(/^Domaines d’e-mail/).fill('neo-maintenance.example, neo-e2e.example');
  await page.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Enregistrer', exact: true })).toHaveCount(0);
  await page.reload();
  await page.getByRole('tab', { name: 'Connexion' }).click();
  const renamed = (button: string) => cardWith(page, RENAMED, button);
  await expect(
    renamed('Désactiver').getByText('Domaines : neo-e2e.example, neo-maintenance.example'),
  ).toBeVisible();
  await expect(renamed('Désactiver').getByText('secret enregistré')).toBeVisible();

  // Nadia is offered it from her address, and comes back signed in: the
  // kept secret served the code's exchange.
  const nadia = await (await newContext()).newPage();
  await nadia.goto('/login');
  await nadia.getByLabel('Adresse e-mail professionnelle').fill(demo.nadia);
  await nadia.getByLabel('Adresse e-mail professionnelle').blur();
  await nadia.getByRole('button', { name: `Se connecter avec ${RENAMED}` }).click();
  await nadia.waitForURL('**/chat');
  const me = await nadia.request.get('/api/auth/me');
  expect(((await me.json()) as { user: { email: string } }).user.email).toBe(demo.nadia);

  // Disabled, it is offered to nobody.
  await renamed('Désactiver').getByRole('button', { name: 'Désactiver', exact: true }).click();
  await expect(renamed('Activer').getByText('Désactivé', { exact: true })).toBeVisible();
  const later = await (await newContext()).newPage();
  await later.goto('/login');
  await later.getByLabel('Adresse e-mail professionnelle').fill(demo.nadia);
  const discovered = later.waitForResponse('**/api/auth/sso/discover');
  await later.getByLabel('Adresse e-mail professionnelle').blur();
  expect(await (await discovered).json()).toEqual({ provider: null });
  await expect(later.getByRole('button', { name: /^Se connecter avec/ })).toHaveCount(0);

  // Deleted (after its confirmation), it is gone from the list as reloaded.
  await renamed('Supprimer').getByRole('button', { name: 'Supprimer', exact: true }).click();
  await expect(page.getByText(RENAMED, { exact: true })).toHaveCount(0);
  await page.reload();
  await page.getByRole('tab', { name: 'Connexion' }).click();
  await expect(page.getByText('Aucun fournisseur d’identité')).toBeVisible();
  await noError(page);
});

test('an invited collaborator activates the account through the provider', async ({
  page,
  newContext,
}) => {
  // An address of this test's own; the account keeps no business role.
  const email = 'ines.morel.sso@neo-maintenance.example';
  await signIn(page, demo.paul);
  // Declared through the API: its screens are the first test's.
  const declared = await page.request.post('/api/admin/identity-providers', {
    headers: WRITE,
    data: {
      kind: 'oidc',
      name: PROVIDER,
      issuer: ISSUER,
      clientId: CLIENT_ID,
      clientSecret: CLIENT_SECRET,
      domains: ['neo-maintenance.example'],
    } satisfies NewIdentityProvider,
  });
  expect(declared.status()).toBe(201);
  const created = await page.request.post('/api/admin/users', {
    headers: WRITE,
    data: { email, displayName: 'Inès Morel' } satisfies NewAccount,
  });
  expect(created.status()).toBe(201);

  // From the e-mailed invitation, activated through the provider.
  const { link } = await lastLink(email, 'invitation');
  const newcomer = await (await newContext()).newPage();
  await newcomer.goto(link);
  await expect(newcomer.getByText('Bienvenue, Inès Morel')).toBeVisible();
  await newcomer.getByRole('button', { name: `Activer mon compte avec ${PROVIDER}` }).click();
  await newcomer.waitForURL('**/chat');
  const me = await newcomer.request.get('/api/auth/me');
  expect(((await me.json()) as { user: { email: string } }).user.email).toBe(email);
  // No password to change: « Mon compte » says so.
  await newcomer.goto('/account');
  await expect(newcomer.getByText('Votre compte n’a pas de mot de passe')).toBeVisible();
  await expect(newcomer.getByLabel('Mot de passe actuel')).toHaveCount(0);
  await noError(newcomer);

  // Active as the API records it; the link spent.
  const listed = await page.request.get('/api/admin/users');
  const { users } = (await listed.json()) as Directory;
  expect(users.find((u) => u.email === email)?.status).toBe('active');
  const later = await (await newContext()).newPage();
  await later.goto(link);
  await expect(later.getByText('Ce lien d’invitation n’est plus valable')).toBeVisible();
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

test('a discovery refused past its limit says so, the password staying', async ({ page }) => {
  // As the API answers past the addresses a network may ask about (ADR-063):
  // the end-to-end server's limit is too high to reach.
  await page.route('**/api/auth/sso/discover', (route) =>
    route.fulfill({
      status: 429,
      contentType: 'application/json',
      body: JSON.stringify({ error: { code: 'rate_limited', message: 'Trop de requêtes' } }),
    }),
  );
  await page.goto('/login');
  await page.getByLabel('Adresse e-mail professionnelle').fill(demo.nadia);
  await page.getByLabel('Adresse e-mail professionnelle').blur();
  await expect(
    page.getByText('Trop d’adresses essayées depuis votre réseau', { exact: false }),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: /^Se connecter avec/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Se connecter', exact: true })).toBeVisible();
});
