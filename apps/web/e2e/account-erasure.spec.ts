// The erasure of a disabled account in the « Utilisateurs » tab (ADR-189):
// - Paul opens an account made for the test: while it is not disabled, no
//   erasure is offered. Disabled, it is; another address typed is refused,
//   and so is the right one while the account leads a unit, each refusal
//   said as the server says it and the account still listed;
// - the unit given another lead, the erasure goes: what went with the
//   account is shown, and the account is gone from the directory, read
//   again after reloading as the API lists it.
// The account a test makes carries an address of its own and is erased by
// the cleanup if the test does not (`support/accounts.ts`); the unit it
// makes is removed even when it fails. The rules themselves are tested on
// the API (`crates/api/tests/account_erasure*.rs`).
import type { Page } from '@playwright/test';
import type {
  Created,
  Directory,
  NewOrgUnit,
  Organization,
  OrgUnitUpdate,
} from '../lib/api/contract';
import { makeAccount, retire, type MadeAccounts } from './support/accounts';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const TAB = 'Utilisateurs';
const WRITE = { 'X-Mimorya-Request': '1' };
const PAUL = 'paul.renaud@neo-maintenance.example';
const ADDRESS = 'Adresse du compte, à retaper pour confirmer';

/** What the test made of accounts, roles and units, for its cleanup. */
const made: MadeAccounts & { paul?: Page; unit?: string } = { roles: [] };

// What the test made, removed even when it fails.
test.afterEach(async () => {
  const paul = made.paul;
  made.paul = undefined;
  if (!paul) return;
  if (made.unit) {
    const removed = await paul.request.delete(`/api/admin/org-units/${made.unit}`, {
      headers: WRITE,
    });
    expect.soft(removed.status()).toBe(200);
    made.unit = undefined;
  }
  await retire(paul, made);
});

/** A team under the company's root, led by `lead`, noted in `made`. */
async function ledTeam(paul: Page, lead: string, name: string): Promise<string> {
  const organization = (await (
    await paul.request.get('/api/admin/organization')
  ).json()) as Organization;
  const root = organization.units.find((unit) => unit.parentId === null);
  expect(root).toBeDefined();
  const created = await paul.request.post('/api/admin/org-units', {
    headers: WRITE,
    data: { parentId: root?.id ?? '', name, type: 'team' } satisfies NewOrgUnit,
  });
  expect(created.status()).toBe(201);
  made.unit = ((await created.json()) as Created).id;
  await setLead(paul, made.unit, lead);
  return made.unit;
}

/** Gives the unit `unit` the lead `lead`, or none. */
async function setLead(paul: Page, unit: string, lead: string | null) {
  const changed = await paul.request.patch(`/api/admin/org-units/${unit}`, {
    headers: WRITE,
    data: { leadUserId: lead } satisfies OrgUnitUpdate,
  });
  expect(changed.status()).toBe(200);
}

/** Whether the directory, as the API lists it, still holds `account`. */
async function listed(paul: Page, account: string): Promise<boolean> {
  const directory = (await (await paul.request.get('/api/admin/users')).json()) as Directory;
  return directory.users.some((user) => user.id === account);
}

test('an administrator erases a disabled account once nothing ties it to others', async ({
  page,
}) => {
  await openAdmin(page, TAB);
  made.paul = page;
  const stamp = Date.now();
  const email = `effacement.${stamp}@neo-maintenance.example`;
  const account = await makeAccount(page, made, { email, displayName: 'Inès Lambert' });
  const unit = await ledTeam(page, account, `Équipe à effacer ${stamp} (e2e)`);

  // The account as the directory lists it once read again: not disabled,
  // nothing to erase.
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  const row = page.getByRole('row').filter({ hasText: email });
  await row.getByRole('button', { name: 'Gérer' }).click();
  await expect(page.getByRole('heading', { name: 'Effacement' })).toHaveCount(0);

  // Disabled, the erasure is offered.
  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: 'Désactiver le compte' }).click();
  await expect(page.getByRole('heading', { name: 'Effacement' })).toBeVisible();
  const typed = page.getByLabel(ADDRESS);
  const erase = page.getByRole('button', { name: 'Effacer le compte' });

  // Another address: refused, the account still there.
  await typed.fill(PAUL);
  await erase.click();
  await expect(page.getByText('L’adresse saisie n’est pas celle du compte')).toBeVisible();
  expect(await listed(page, account)).toBe(true);

  // Its own, while it leads a unit: refused as the server says it.
  await typed.fill(email.toUpperCase());
  await erase.click();
  await expect(page.getByText('Ce compte dirige une unité')).toBeVisible();
  expect(await listed(page, account)).toBe(true);

  // Another lead given, erased: what went with it is shown, and it is gone.
  await setLead(page, unit, null);
  await erase.click();
  await expect(page.getByText('Compte de Inès Lambert effacé')).toBeVisible();
  await expect(
    page.getByText(
      'Sont partis avec lui 0 conversation (0 message), 0 mémoire personnelle et 0 demande faite pour lui.',
    ),
  ).toBeVisible();
  made.account = undefined;
  made.email = undefined;
  await expect(row).toHaveCount(0);
  expect(await listed(page, account)).toBe(false);
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(page.getByRole('row').filter({ hasText: 'Paul Renaud' })).toBeVisible();
  await expect(page.getByRole('row').filter({ hasText: email })).toHaveCount(0);
  await noError(page);
});
