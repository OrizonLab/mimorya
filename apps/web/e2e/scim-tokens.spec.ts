// The SCIM tokens of the « Utilisateurs » tab (ADR-183, ADR-184):
// - Paul, who manages the company's accounts, makes a token named for the
//   identity provider. Its secret is shown this once, with the SCIM address
//   to give the provider, and reads the company's accounts there; the notice
//   closed, the focus is on the section's title. Read again after reloading,
//   the token is listed by its name, its last use noted, its secret gone.
//   Removed after a confirmation, it reads nothing more;
// - the notice of a new token stands outside the directory's reading: while
//   the directory is read again after a change, its buttons still work;
//   when that reading fails, the notice and its secret stay;
// - an account that only reads the directory (`user:read`, company-wide)
//   sees the accounts, neither the making of one nor the SCIM tokens.
// Every token a test makes carries `MARK`, removed even when it fails; the
// account a test makes keeps no role and is disabled (`support/accounts.ts`).
// The rules themselves are tested on the API (`crates/api/tests/scim_*.rs`);
// the section's behaviour on refusals is the export tokens' (`BearerTokens`,
// `audit-export.spec.ts`).
import type { Page } from '@playwright/test';
import type { ErrorBody, ScimTokens, ScimUserList } from '../lib/api/contract';
import {
  activate,
  grantCompanyWide,
  makeAccount,
  makeRole,
  retire,
  type MadeAccounts,
} from './support/accounts';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';
import { holdList } from './support/hold';

const TAB = 'Utilisateurs';
const TITLE = 'Provisionnement SCIM';
const TOKENS = '/api/admin/scim/tokens';
const WRITE = { 'X-Mimorya-Request': '1' };
const MARK = '(bout en bout)';
const NAME = `Microsoft Entra ID ${MARK}`;
const PAUL = 'paul.renaud@neo-maintenance.example';
const CHOSEN = 'phrase-de-passe-scim-e2e';
/** What the API answers when the directory cannot be read. */
const FAILED = {
  error: { code: 'internal', message: 'Annuaire momentanément illisible' },
} satisfies ErrorBody;

/** What the test made of accounts and roles, for its cleanup. */
const made: MadeAccounts & { paul?: Page } = { roles: [] };

// What the tests made, removed even when they fail; 401: never signed in.
test.afterEach(async ({ page }) => {
  const paul = made.paul;
  made.paul = undefined;
  if (paul) await retire(paul, made);
  const listed = await page.request.get(TOKENS);
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { tokens } = (await listed.json()) as ScimTokens;
  for (const token of tokens.filter((t) => t.name.endsWith(MARK))) {
    const removed = await page.request.delete(`${TOKENS}/${token.id}`, { headers: WRITE });
    expect(removed.status()).toBe(200);
  }
});

/** Opens the tab afresh as `page`, its lists loaded. */
async function openTab(page: Page) {
  await page.goto('/admin');
  await page.getByRole('tab', { name: TAB }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
}

test('an administrator makes a SCIM token, shown once, then removes it', async ({ page }) => {
  await openAdmin(page, TAB);
  const title = page.getByRole('heading', { name: TITLE });
  await expect(title).toBeVisible();
  const address = page.getByText(/\/api\/scim\/v2$/);
  await expect(address).toBeVisible();
  const scimUrl = (await address.innerText()).trim();
  expect(scimUrl).toBe('http://localhost:3000/api/scim/v2');

  await page.getByLabel(/Nom du nouveau jeton/).fill(NAME);
  await page.getByRole('button', { name: 'Créer un jeton' }).click();
  const notice = page.getByRole('region', { name: `Jeton « ${NAME} »` });
  await expect(notice).toBeVisible();
  await expect(notice.getByText(scimUrl, { exact: true })).toBeVisible();
  const secret = await notice.getByLabel(/Jeton secret/).inputValue();
  expect(secret).toMatch(/^msc_[A-Za-z0-9_-]{40,}$/);
  await noError(page);

  // The identity provider reads the company's accounts with it, at the
  // address shown.
  const read = (token: string) =>
    page.request.get(`${scimUrl}/Users?filter=userName%20eq%20%22${encodeURIComponent(PAUL)}%22`, {
      headers: { Authorization: `Bearer ${token}` },
    });
  const found = await read(secret);
  expect(found.status()).toBe(200);
  const accounts = (await found.json()) as ScimUserList;
  expect(accounts.totalResults).toBe(1);
  expect(accounts.Resources[0]?.userName).toBe(PAUL);

  // Noted, the notice goes; read again, the token is listed, used, its secret
  // shown nowhere.
  await notice.getByRole('button', { name: 'J’ai noté le jeton' }).click();
  await expect(notice).toHaveCount(0);
  await expect(title).toBeFocused();
  await openTab(page);
  const row = page.getByRole('row').filter({ hasText: NAME });
  await expect(row).toBeVisible();
  await expect(row.getByText('Jamais')).toHaveCount(0);
  await expect(page.getByText(secret)).toHaveCount(0);

  // Removed after a confirmation, it reads nothing more.
  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: `Retirer le jeton ${NAME}` }).click();
  await expect(row).toHaveCount(0);
  // Its button gone with its row, the focus is back on the section's title.
  await expect(title).toBeFocused();
  await openTab(page);
  await expect(page.getByRole('row').filter({ hasText: NAME })).toHaveCount(0);
  expect((await read(secret)).status()).toBe(401);
  await noError(page);
});

test('a new token’s notice is neither locked nor lost by the directory read again', async ({
  page,
}) => {
  await openAdmin(page, TAB);
  made.paul = page;
  const email = `fonction.scim.${Date.now()}@neo-maintenance.example`;
  await makeAccount(page, made, { email, displayName: 'Alix Morel' });
  await openTab(page);
  await page.getByLabel(/Nom du nouveau jeton/).fill(NAME);
  await page.getByRole('button', { name: 'Créer un jeton' }).click();
  const notice = page.getByRole('region', { name: `Jeton « ${NAME} »` });
  const secret = await notice.getByLabel(/Jeton secret/).inputValue();
  const copy = notice.getByRole('button', { name: 'Copier le jeton' });
  const noted = notice.getByRole('button', { name: 'J’ai noté le jeton' });

  // The directory read again after a change, held: the directory is locked,
  // not the notice.
  await page
    .getByRole('row')
    .filter({ hasText: email })
    .getByRole('button', { name: 'Gérer' })
    .click();
  const jobTitle = page.getByLabel('Fonction');
  const save = page.getByRole('button', { name: 'Enregistrer', exact: true });
  const directory = await holdList(page, '/api/admin/users');
  const reload = directory.arm();
  await jobTitle.fill('Technicienne');
  await save.click();
  await reload;
  await expect(jobTitle).toBeDisabled();
  await expect(copy).toBeEnabled();
  await expect(noted).toBeEnabled();
  directory.release();
  await expect(jobTitle).toBeEnabled();

  // Read again and failed: the directory says so; the notice and its secret
  // stay.
  await page.route('**/api/admin/users', (route) =>
    route.request().method() === 'GET'
      ? route.fulfill({ status: 500, json: FAILED })
      : route.fallback(),
  );
  await jobTitle.fill('Cheffe d’équipe');
  await save.click();
  await expect(page.getByText(FAILED.error.message)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Réessayer' })).toBeVisible();
  await expect(notice.getByLabel(/Jeton secret/)).toHaveValue(secret);
  await expect(copy).toBeEnabled();
  await expect(noted).toBeEnabled();
});

test('who only reads the directory sees neither the making of an account nor the SCIM tokens', async ({
  page,
  newContext,
}) => {
  await openAdmin(page, TAB);
  made.paul = page;
  const stamp = Date.now();
  const reading = await makeRole(page, made, {
    name: `Lecture de l’annuaire ${stamp} (e2e)`,
    description: 'Rôle du test des jetons SCIM',
    permissions: ['user:read'],
  });
  const email = `annuaire.scim.${stamp}@neo-maintenance.example`;
  const account = await makeAccount(page, made, { email, displayName: 'Hugo Perrin' });
  await grantCompanyWide(page, account, reading);

  const hugo = await (await newContext()).newPage();
  await activate(hugo, email, CHOSEN);
  await openTab(hugo);
  await expect(hugo.getByRole('row').filter({ hasText: 'Paul Renaud' })).toBeVisible();
  await expect(hugo.getByRole('heading', { name: TITLE })).toHaveCount(0);
  await expect(hugo.getByRole('button', { name: 'Créer un jeton' })).toHaveCount(0);
  await expect(hugo.getByRole('button', { name: 'Ajouter un collaborateur' })).toHaveCount(0);
  expect((await hugo.request.get(TOKENS)).status()).toBe(403);
  await noError(hugo);
});
