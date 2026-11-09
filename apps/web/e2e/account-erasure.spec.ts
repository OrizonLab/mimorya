// The erasure of a disabled account in the « Utilisateurs » tab (ADR-190):
// - Paul opens an account made for the test: while it is not disabled, no
//   erasure is offered. Disabled, it is; another address typed is refused,
//   and so is the right one while the account leads a unit, each refusal
//   said as the server says it and the account still listed;
// - the unit given another lead, the erasure goes: what went with the
//   account is shown, the notice taking the focus, and the account is gone
//   from the directory, read again after reloading as the API lists it;
// - who only erases accounts opens no active account, and of a disabled
//   one, which kept two conversations and two memories, the erasure alone:
//   erased, what went is said in words agreeing with its counts.
// The accounts a test makes carry addresses of their own and are erased by
// the cleanup if the test does not (`support/accounts.ts`); the unit it
// makes is removed even when it fails. The rules themselves are tested on
// the API (`crates/api/tests/account_erasure*.rs`, `admin_directory.rs`).
import type { Page } from '@playwright/test';
import type {
  AccountUpdate,
  Created,
  Directory,
  ManualMemory,
  NewConversation,
  NewOrgUnit,
  Organization,
  OrgUnitUpdate,
} from '../lib/api/contract';
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

const TAB = 'Utilisateurs';
const WRITE = { 'X-Mimorya-Request': '1' };
const PAUL = 'paul.renaud@neo-maintenance.example';
const ADDRESS = 'Adresse du compte, à retaper pour confirmer';
const CHOSEN = 'phrase-de-passe-effacement-e2e';

/** What the test made of accounts, roles and units, for its cleanup. */
const made: MadeAccounts & { paul?: Page; unit?: string } = { roles: [] };
/** The account a test erases, when it is not the one in `made`. */
const departing: MadeAccounts = { roles: [] };

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
  await retire(paul, departing);
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
  await expect(page.getByRole('heading', { name: 'Compte effacé : Inès Lambert' })).toBeFocused();
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

test('who only erases accounts is offered the erasure of a disabled account, and nothing more', async ({
  page,
  newContext,
}) => {
  await openAdmin(page, TAB);
  made.paul = page;
  const stamp = Date.now();
  const erasing = await makeRole(page, made, {
    name: `Effacement des comptes ${stamp} (e2e)`,
    description: 'Rôle du test de l’effacement des comptes',
    permissions: ['user:erase'],
  });
  const eraserEmail = `effaceur.${stamp}@neo-maintenance.example`;
  const eraser = await makeAccount(page, made, { email: eraserEmail, displayName: 'Noé Garnier' });
  await grantCompanyWide(page, eraser, erasing);

  // The account to erase keeps two conversations and two memories.
  const email = `a.effacer.${stamp}@neo-maintenance.example`;
  const account = await makeAccount(page, departing, { email, displayName: 'Camille Roux' });
  const camille = await (await newContext()).newPage();
  await activate(camille, email, CHOSEN);
  for (const title of ['Planning de la semaine', 'Relevés du chantier']) {
    const opened = await camille.request.post('/api/conversations', {
      headers: WRITE,
      data: { title } satisfies NewConversation,
    });
    expect(opened.status()).toBe(201);
  }
  for (const title of ['Préférence horaire', 'Outil habituel']) {
    const noted = await camille.request.post('/api/memory', {
      headers: WRITE,
      data: {
        space: 'personal',
        kind: 'preference',
        title,
        content: 'Note de test',
      } satisfies ManualMemory,
    });
    expect(noted.status()).toBe(201);
  }
  // Her page closed before her account goes: nothing of it is read again.
  await camille.context().close();
  const disabled = await page.request.patch(`/api/admin/users/${account}`, {
    headers: WRITE,
    data: { status: 'disabled' } satisfies AccountUpdate,
  });
  expect(disabled.status()).toBe(200);

  // Noé opens no active account, nor makes one.
  const noe = await (await newContext()).newPage();
  await activate(noe, eraserEmail, CHOSEN);
  await noe.goto('/admin');
  await noe.getByRole('tab', { name: TAB }).click();
  const paulRow = noe.getByRole('row').filter({ hasText: 'Paul Renaud' });
  await expect(paulRow).toBeVisible();
  await expect(paulRow.getByRole('button', { name: 'Gérer' })).toHaveCount(0);
  await expect(noe.getByRole('button', { name: 'Ajouter un collaborateur' })).toHaveCount(0);

  // Of the disabled account, the erasure alone.
  const row = noe.getByRole('row').filter({ hasText: 'Camille Roux' });
  await expect(row).toContainText(email);
  await row.getByRole('button', { name: 'Gérer' }).click();
  await expect(noe.getByRole('heading', { name: 'Effacement' })).toBeVisible();
  for (const section of ['Attributs', 'Absence', 'Habilitations']) {
    await expect(noe.getByRole('heading', { name: section })).toHaveCount(0);
  }
  await expect(noe.getByRole('button', { name: 'Réactiver le compte' })).toHaveCount(0);
  await expect(noe.getByRole('button', { name: 'Enregistrer', exact: true })).toHaveCount(0);

  // Erased: what went, in words agreeing with its counts.
  await noe.getByLabel(ADDRESS).fill(email);
  await noe.getByRole('button', { name: 'Effacer le compte' }).click();
  await expect(noe.getByRole('heading', { name: 'Compte effacé : Camille Roux' })).toBeFocused();
  await expect(
    noe.getByText(
      'Sont partis avec lui 2 conversations (0 message), 2 mémoires personnelles et 0 demande faite pour lui.',
    ),
  ).toBeVisible();
  departing.account = undefined;
  departing.email = undefined;
  await expect(row).toHaveCount(0);
  expect(await listed(page, account)).toBe(false);
  await noError(noe);
});
