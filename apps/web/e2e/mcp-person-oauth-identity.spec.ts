// A server each person authorizes, whose authorization server must prove
// that each authorizes with their own account (ADR-175): declared so in
// « Connecteurs », said on its card and in each person's account. Marc,
// signed in there as himself, authorizes it; Lucas, signed in there as
// Marc, is refused, and told why. The end-to-end API serves this
// authorization server, an OpenID provider where Marc is signed in. The
// server added is deleted, with the authorizations given, even when the
// test fails.
import type { Page } from '@playwright/test';
import type { PersonAuthorizations } from '../lib/api/contract';
import { addServer, cardWith, deleteConnectorsNamed, openAdmin } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import {
  OAUTH_CLIENT_ID as CLIENT_ID,
  OAUTH_CLIENT_SECRET as CLIENT_SECRET,
  connectorNamed,
} from './support/oauth';

const SERVER = 'Fichiers vérifiés e2e';
/** Protected by an OpenID provider where Marc is signed in. */
const ADDRESS = 'http://127.0.0.1:4111/mcp';
const VERIFY = /^Vérifier que chacun l’autorise avec son propre compte/;

test.afterEach(async ({ page }) => {
  await deleteConnectorsNamed(page, [SERVER], [401, 403]);
});

/** The person's own view of the server, as their account lists it. */
async function listedFor(page: Page) {
  const response = await page.request.get('/api/account/connector-authorizations');
  expect(response.status()).toBe(200);
  const { servers } = (await response.json()) as PersonAuthorizations;
  return servers.find((server) => server.name === SERVER);
}

/** From the person's account, there and back from the authorization server. */
async function authorizeFromAccount(page: Page) {
  await page.goto('/account');
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  const row = page.getByRole('region', { name: SERVER });
  await expect(row.getByText('Compte vérifié', { exact: true })).toBeVisible();
  await row.getByRole('button', { name: /^Autoriser/ }).click();
}

test('each person must authorize with their own account', async ({ page, newContext }) => {
  await openAdmin(page, 'Connecteurs');
  await addServer(page, SERVER, 'verifie', ADDRESS);
  const card = cardWith(page, SERVER, 'Supprimer');
  await card.getByRole('button', { name: 'Déclarer OAuth' }).click();
  await card.getByLabel('Identifiant du client').fill(CLIENT_ID);
  await card.getByLabel(/^Secret du client/).fill(CLIENT_SECRET);
  // Offered only once each person authorizes it.
  await expect(card.getByLabel(VERIFY)).toHaveCount(0);
  await card.getByLabel('Chaque personne, pour elle-même, dans « Mon compte »').check();
  await card.getByLabel(VERIFY).check();
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
  await expect(card.getByText('Compte vérifié', { exact: true })).toBeVisible();
  await expect(card.getByText(/^Son serveur d’autorisation prouve que chacun/)).toBeVisible();
  const declared = await connectorNamed(page, SERVER);
  expect(declared?.mcp?.oauth?.verifiesIdentity).toBe(true);
  expect(declared?.mcp?.oauth?.scopes).toEqual(expect.arrayContaining(['openid', 'email']));
  await noError(page);

  // Marc, signed in there as himself: authorized.
  const marc = await (await newContext()).newPage();
  await signIn(marc, demo.marc);
  await authorizeFromAccount(marc);
  await expect(
    marc.getByText(/^Serveur autorisé : l’agent l’utilise désormais pour vous/),
  ).toBeVisible();
  const his = await listedFor(marc);
  expect(his?.verifiesIdentity).toBe(true);
  expect(his?.authorization).not.toBeNull();
  await noError(marc);

  // Lucas, signed in there as Marc: refused, and told why, once.
  const lucas = await (await newContext()).newPage();
  await signIn(lucas, demo.lucas);
  await authorizeFromAccount(lucas);
  await expect(
    lucas.getByText(/^Le compte choisi chez le serveur d’autorisation n’est pas le vôtre/),
  ).toBeVisible();
  await expect(lucas).toHaveURL(/\/account$/);
  const row = lucas.getByRole('region', { name: SERVER });
  await expect(row.getByText('Non autorisé', { exact: true })).toBeVisible();
  expect((await listedFor(lucas))?.authorization).toBeNull();
  // Said once; his account still says the renewal failure written for
  // him at the start of the end-to-end API (`person_renewal.rs`).
  await lucas.reload();
  await expect(lucas.getByText('Chargement…')).toHaveCount(0);
  await expect(lucas.getByText(/n’est pas le vôtre/)).toHaveCount(0);
});
