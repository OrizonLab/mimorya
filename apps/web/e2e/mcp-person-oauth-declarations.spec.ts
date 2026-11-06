// An MCP server's OAuth declared again with the other holder (ADR-174):
// the confirmation says what goes, every person's authorization when the
// company is to authorize it, the company's when each person is, and the
// declaration then read back from the API. The server added is deleted,
// even when the test fails.
import type { Page } from '@playwright/test';
import {
  addServer,
  cardWith,
  connectorsTab,
  deleteConnectorsNamed,
  openAdmin,
} from './support/admin';
import { expect, noError, test } from './support/fixtures';
import {
  OAUTH_CLIENT_ID as CLIENT_ID,
  OAUTH_CLIENT_SECRET as CLIENT_SECRET,
  OAUTH_SERVER as ADDRESS,
  connectorNamed,
} from './support/oauth';

const SERVER = 'Fichiers redéclarés e2e';
const EACH = 'Chaque personne, pour elle-même, dans « Mon compte »';
const COMPANY = 'L’entreprise, une fois pour tous les runs';

/** The confirmations asked so far in the test, in order. */
let confirmations: string[] = [];

test.beforeEach(({ page }) => {
  confirmations = [];
  page.on('dialog', (dialog) => {
    confirmations.push(dialog.message());
    void dialog.accept();
  });
});

test.afterEach(async ({ page }) => {
  await deleteConnectorsNamed(page, [SERVER], [401]);
});

/** Declares the server again, `holder` chosen, its secret typed anew. */
async function declareAgain(page: Page, holder: string) {
  const card = cardWith(page, SERVER, 'Supprimer');
  await card.getByRole('button', { name: 'Modifier la déclaration' }).click();
  await card.getByLabel(holder).check();
  await card.getByLabel(/^Secret du client/).fill(CLIENT_SECRET);
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
}

test('declaring again with the other holder says what it takes', async ({ page }) => {
  await openAdmin(page, 'Connecteurs');
  await addServer(page, SERVER, 'redecl', ADDRESS);
  const card = cardWith(page, SERVER, 'Supprimer');
  await card.getByRole('button', { name: 'Déclarer OAuth' }).click();
  await card.getByLabel('Identifiant du client').fill(CLIENT_ID);
  await card.getByLabel(/^Secret du client/).fill(CLIENT_SECRET);
  await card.getByLabel(EACH).check();
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
  await expect(card.getByText('Chaque personne l’autorise', { exact: true })).toBeVisible();

  // Paul's own authorization, then declared again for the company: the
  // confirmation says the people's go, the company to authorize it.
  await page.goto('/account');
  const own = page.getByRole('region', { name: SERVER });
  await own.getByRole('button', { name: 'Autoriser', exact: true }).click();
  await expect(page.getByText(/^Serveur autorisé : /)).toBeVisible();
  await connectorsTab(page);
  await expect(card.getByText('1 personne l’a autorisé pour elle-même.')).toBeVisible();
  await declareAgain(page, COMPANY);
  expect(confirmations.at(-1)).toContain(
    'Déclarer de nouveau retire les autorisations des personnes (1) : l’entreprise autorisera ensuite ce serveur pour tous les runs.',
  );
  await expect(card.getByText('Non autorisé', { exact: true })).toBeVisible();
  const company = await connectorNamed(page, SERVER);
  expect(company?.mcp?.oauth?.perPerson).toBe(false);
  expect(company?.mcp?.oauth?.personAuthorizations).toBe(0);

  // Authorized for the company, then declared again for each person: the
  // company's goes, each person to authorize it.
  await card.getByRole('button', { name: 'Autoriser', exact: true }).click();
  await expect(page.getByText('Serveur MCP autorisé')).toBeVisible();
  await expect(card.getByText(/^Autorisé le /)).toBeVisible();
  await declareAgain(page, EACH);
  expect(confirmations.at(-1)).toContain(
    'Déclarer de nouveau retire l’autorisation de l’entreprise : chaque personne autorisera ensuite ce serveur pour elle-même.',
  );
  await expect(card.getByText('Chaque personne l’autorise', { exact: true })).toBeVisible();
  const each = await connectorNamed(page, SERVER);
  expect(each?.mcp?.oauth?.perPerson).toBe(true);
  expect(each?.mcp?.oauth?.authorization).toBeNull();
  await noError(page);
});
