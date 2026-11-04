// An MCP server protected by OAuth, from the administration (ADR-171):
// added at its address, declared with the client registered at its
// authorization server, which the platform finds from the server itself,
// authorized there and back, its session started with the authorization's
// token, then its authorization and its declaration withdrawn. A return
// that failed is said; a declaration that no longer reads is withdrawn from
// its card. The end-to-end API serves both servers and reaches them alone
// of the private network (`e2e_server`). What the test adds is deleted,
// even when it fails.
import type { Page } from '@playwright/test';
import type { Connectors } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const SERVER = 'Fichiers OAuth e2e';
const ADDRESS = 'http://127.0.0.1:4107/mcp';
const ISSUER = 'http://127.0.0.1:4106';
/** The client the authorization server knows, with its secret. */
const CLIENT_ID = 'mimorya-test-client';
const CLIENT_SECRET = 'secret-du-client-de-test';
const RETURN_ADDRESS = 'http://localhost:3000/api/admin/connectors/oauth/callback';
/** Left by the end-to-end API at its start, disabled. */
const UNREADABLE = 'Serveur OAuth illisible';
const WRITE = { 'X-Mimorya-Request': '1' };

/** The company's connectors, as the API lists them. */
async function listed(page: Page) {
  const response = await page.request.get('/api/admin/connectors');
  expect(response.status()).toBe(200);
  return ((await response.json()) as Connectors).instances;
}

/** The confirmations asked so far in the test, in order. */
let confirmations: string[] = [];

test.beforeEach(({ page }) => {
  // Withdrawals and deletions ask for a confirmation.
  confirmations = [];
  page.on('dialog', (dialog) => {
    confirmations.push(dialog.message());
    void dialog.accept();
  });
});

test.afterEach(async ({ page }) => {
  // 401: the test never signed in, so added nothing.
  const response = await page.request.get('/api/admin/connectors');
  if (response.status() === 401) return;
  for (const connector of await listed(page)) {
    if (connector.name !== SERVER) continue;
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

test('an MCP server is authorized by OAuth, started with its token, then withdrawn', async ({
  page,
}) => {
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Ajouter un serveur MCP' }).click();
  await page.getByLabel('Nom').fill(SERVER);
  await page.getByLabel('Préfixe des outils (définitif)').fill('fichoa');
  await page.getByLabel(/^Adresse du serveur/).fill(ADDRESS);
  await page.getByRole('button', { name: 'Ajouter le serveur' }).click();

  // Declared with the client registered at the authorization server, the
  // address to register said first.
  const card = cardWith(page, SERVER, 'Supprimer');
  await card.getByRole('button', { name: 'Déclarer OAuth' }).click();
  await expect(card.getByText(RETURN_ADDRESS, { exact: true })).toBeVisible();
  await card.getByLabel('Identifiant du client').fill(CLIENT_ID);
  await card.getByLabel(/^Secret du client/).fill(CLIENT_SECRET);
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
  // Found from the server itself, not yet authorized: said so in its summary.
  await expect(card.getByText(ISSUER, { exact: true })).toBeVisible();
  await expect(card.getByText('Non autorisé', { exact: true })).toBeVisible();
  await expect(card.getByText(/· OAuth déclaré, non autorisé$/)).toBeVisible();

  // There and back: the browser comes back to the connectors, authorized.
  await card.getByRole('button', { name: 'Autoriser', exact: true }).click();
  await expect(page.getByText('Serveur MCP autorisé')).toBeVisible();
  await expect(page).toHaveURL(/\/admin$/);
  await expect(card.getByText(/^Autorisé le /)).toBeVisible();
  await expect(card.getByText(/· autorisé par OAuth$/)).toBeVisible();
  const authorized = (await listed(page)).find((c) => c.name === SERVER);
  expect(authorized?.mcp?.tokenStored).toBe(false);
  expect(authorized?.mcp?.oauth?.issuer).toBe(ISSUER);
  expect(authorized?.mcp?.oauth?.clientSecretStored).toBe(true);
  expect(authorized?.mcp?.oauth?.authorization?.renewable).toBe(true);
  expect(JSON.stringify(authorized)).not.toContain(CLIENT_SECRET);

  // Its session starts with the authorization's token.
  await card.getByRole('button', { name: 'Tester la connexion' }).click();
  await expect(
    card.getByText(/^Connexion réussie : [1-9]\d* outil\(s\) exposé\(s\)/),
  ).toBeVisible();

  // The authorization withdrawn, then the declaration: back to a token.
  await card.getByRole('button', { name: 'Retirer l’autorisation' }).click();
  await expect(card.getByText('Non autorisé', { exact: true })).toBeVisible();
  // Its server names a revocation endpoint: the confirmation says it is asked.
  expect(confirmations.at(-1)).toContain('demande au serveur d’autorisation de révoquer');
  const unauthorized = (await listed(page)).find((c) => c.name === SERVER);
  expect(unauthorized?.mcp?.oauth?.authorization).toBeNull();
  await card.getByRole('button', { name: 'Retirer OAuth' }).click();
  await expect(card.getByRole('button', { name: 'Déclarer OAuth' })).toBeVisible();
  const withdrawn = (await listed(page)).find((c) => c.name === SERVER);
  expect(withdrawn?.mcp?.oauth).toBeNull();
  await noError(page);
});

test('a return that failed is said on the connectors', async ({ page }) => {
  await openAdmin(page, 'Connecteurs');
  await page.goto('/admin?autorisation=echec');
  const failed = page.getByText(/^L’autorisation du serveur MCP n’a pas abouti/);
  await expect(failed).toBeVisible();
  // Said once: the address no longer carries it.
  await expect(page).toHaveURL(/\/admin$/);
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await expect(page.getByRole('button', { name: 'Ajouter un serveur MCP' })).toBeVisible();
  await expect(failed).toHaveCount(0);
  await noError(page);
});

test('a declaration that no longer reads is withdrawn, then its server deleted', async ({
  page,
}) => {
  await openAdmin(page, 'Connecteurs');
  const card = cardWith(page, UNREADABLE, 'Supprimer');
  await expect(card.getByText(/^Déclaration OAuth illisible/)).toBeVisible();
  // A state of the server, not an alert.
  await noError(page);
  await card.getByRole('button', { name: 'Retirer la déclaration OAuth' }).click();
  await expect(card.getByRole('button', { name: 'Déclarer OAuth' })).toBeVisible();
  const repaired = (await listed(page)).find((c) => c.name === UNREADABLE);
  expect(repaired?.oauthUnreadable).toBe(false);
  expect(repaired?.mcp?.url).toBe('https://mcp.illisible.example/mcp');
  await card.getByRole('button', { name: 'Supprimer' }).click();
  await expect(page.getByText(UNREADABLE)).toHaveCount(0);
  expect((await listed(page)).some((c) => c.name === UNREADABLE)).toBe(false);
  await noError(page);
});
