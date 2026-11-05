// An MCP server protected by OAuth, from the administration (ADR-171):
// added at its address, declared with the client registered at its
// authorization server, which the platform finds from the server itself,
// authorized there and back, its session started with the authorization's
// token, then its authorization and its declaration withdrawn; one whose
// authorization server names no revocation endpoint says what is left to
// do there (ADR-172); one authorized with the client the platform
// registered itself there (ADR-173). A return that failed is said; a declaration that no
// longer reads is withdrawn from its card. The end-to-end API serves these
// servers and reaches them alone of the private network (`e2e_server`).
// What the test adds is deleted, even when it fails.
import type { Page } from '@playwright/test';
import type { Connectors } from '../lib/api/contract';
import { addServer, cardWith, openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const SERVER = 'Fichiers OAuth e2e';
const ADDRESS = 'http://127.0.0.1:4107/mcp';
/** A server whose authorization server names no revocation endpoint. */
const UNREVOKING = 'Fichiers OAuth sans révocation e2e';
const UNREVOKING_ADDRESS = 'http://127.0.0.1:4109/mcp';
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
    if (connector.name !== SERVER && connector.name !== UNREVOKING) continue;
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
  await addServer(page, SERVER, 'fichoa', ADDRESS);

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
  // A client the company typed: nothing said of one the platform registered.
  expect(confirmations.at(-1)).toContain('Retirer OAuth de ce serveur');
  expect(confirmations.at(-1)).not.toContain('reste chez le serveur d’autorisation');
  const withdrawn = (await listed(page)).find((c) => c.name === SERVER);
  expect(withdrawn?.mcp?.oauth).toBeNull();
  await noError(page);
});

test('a server whose authorization server cannot revoke says what is left to do', async ({
  page,
}) => {
  await openAdmin(page, 'Connecteurs');
  await addServer(page, UNREVOKING, 'fichsr', UNREVOKING_ADDRESS);
  const card = cardWith(page, UNREVOKING, 'Supprimer');
  await card.getByRole('button', { name: 'Déclarer OAuth' }).click();
  await card.getByLabel('Identifiant du client').fill(CLIENT_ID);
  await card.getByLabel(/^Secret du client/).fill(CLIENT_SECRET);
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
  // No revocation endpoint known: said, with what is left to do there.
  await expect(card.getByText(/^Aucun point de révocation n’est connu/)).toBeVisible();
  const declared = (await listed(page)).find((c) => c.name === UNREVOKING);
  expect(declared?.mcp?.oauth?.revocable).toBe(false);
  await card.getByRole('button', { name: 'Autoriser', exact: true }).click();
  await expect(page.getByText('Serveur MCP autorisé')).toBeVisible();
  await expect(card.getByText(/^Autorisé le /)).toBeVisible();
  // Withdrawn: the confirmation says the agreement is to withdraw there too.
  await card.getByRole('button', { name: 'Retirer l’autorisation' }).click();
  await expect(card.getByText('Non autorisé', { exact: true })).toBeVisible();
  expect(confirmations.at(-1)).toContain('retirez aussi l’accord chez lui');
  await noError(page);
});

test('an MCP server is authorized with the client the platform registers itself', async ({
  page,
}) => {
  await openAdmin(page, 'Connecteurs');
  await addServer(page, SERVER, 'fichin', ADDRESS);

  // The platform registers itself: no client to type.
  const card = cardWith(page, SERVER, 'Supprimer');
  await card.getByRole('button', { name: 'Déclarer OAuth' }).click();
  await card
    .getByLabel('La plateforme s’enregistre elle-même auprès du serveur d’autorisation')
    .check();
  await expect(card.getByLabel('Identifiant du client')).toHaveCount(0);
  await expect(card.getByLabel(/^Secret du client/)).toHaveCount(0);
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
  // Said as its own, with when its secret expires.
  await expect(
    card.getByText(/\(enregistré par la plateforme, secret enregistré, secret valable jusqu’au /),
  ).toBeVisible();
  const declared = (await listed(page)).find((c) => c.name === SERVER);
  expect(declared?.mcp?.oauth?.registered).toBe(true);
  expect(declared?.mcp?.oauth?.clientId).not.toBe(CLIENT_ID);
  expect(declared?.mcp?.oauth?.clientSecretStored).toBe(true);
  expect(declared?.mcp?.oauth?.clientSecretExpiresAt).not.toBeNull();

  // Authorized there and back with it.
  await card.getByRole('button', { name: 'Autoriser', exact: true }).click();
  await expect(page.getByText('Serveur MCP autorisé')).toBeVisible();
  await expect(card.getByText(/^Autorisé le /)).toBeVisible();
  const authorized = (await listed(page)).find((c) => c.name === SERVER);
  expect(authorized?.mcp?.oauth?.authorization).not.toBeNull();

  // Declared again: its own client chosen, nothing of it typed; by hand,
  // its identifier is not offered as a typed client's.
  await card.getByRole('button', { name: 'Modifier la déclaration' }).click();
  const own = card.getByLabel(
    'La plateforme s’enregistre elle-même auprès du serveur d’autorisation',
  );
  await expect(own).toBeChecked();
  await expect(card.getByLabel('Identifiant du client')).toHaveCount(0);
  await card.getByLabel('Client enregistré à la main').check();
  await expect(card.getByLabel('Identifiant du client')).toHaveValue('');
  // Its own again: declaring says first what it leaves, then registers anew.
  await own.check();
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
  await expect(card.getByText('Non autorisé', { exact: true })).toBeVisible();
  expect(confirmations.at(-1)).toContain('retire l’autorisation donnée');
  expect(confirmations.at(-1)).toContain('reste chez le serveur d’autorisation');
  const again = (await listed(page)).find((c) => c.name === SERVER);
  expect(again?.mcp?.oauth?.registered).toBe(true);
  expect(again?.mcp?.oauth?.clientId).not.toBe(declared?.mcp?.oauth?.clientId);
  expect(again?.mcp?.oauth?.authorization).toBeNull();

  // Its address to change: the form says the client would stay there.
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  await expect(
    card.getByText(/Changer son adresse.*resterait alors chez le serveur d’autorisation/),
  ).toBeVisible();
  await card.getByRole('button', { name: 'Annuler' }).click();

  // Withdrawn: the client registered stays at the server, said so.
  await card.getByRole('button', { name: 'Retirer OAuth' }).click();
  await expect(card.getByRole('button', { name: 'Déclarer OAuth' })).toBeVisible();
  expect(confirmations.at(-1)).toContain('Retirer OAuth de ce serveur');
  expect(confirmations.at(-1)).toContain('reste chez le serveur d’autorisation');

  // Its own client once more, then the server deleted: said there too.
  await card.getByRole('button', { name: 'Déclarer OAuth' }).click();
  await own.check();
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
  await expect(card.getByText(/\(enregistré par la plateforme/)).toBeVisible();
  await card.getByRole('button', { name: 'Supprimer', exact: true }).click();
  await expect(page.getByText(SERVER)).toHaveCount(0);
  expect(confirmations.at(-1)).toContain(`Supprimer le serveur « ${SERVER} »`);
  expect(confirmations.at(-1)).toContain('reste chez le serveur d’autorisation');
  expect((await listed(page)).some((c) => c.name === SERVER)).toBe(false);
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
