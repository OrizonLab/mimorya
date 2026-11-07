// Who opens the « Journal d’audit » tab, and what they see there (ADR-182).
// An account made for the test, holding only the export of the log
// (`audit:export`, company-wide), opens the tab and sees the SIEM's tokens
// alone, neither the log nor its verification. Given the reading of the log
// instead (`audit:read`), it sees the log and its verification, not the
// tokens. Whatever happens, the account keeps no role and is disabled, and
// the test's roles are deleted.
import type { Page } from '@playwright/test';
import type { AccountUpdate, Directory, NewAccount, NewRole, RoleGrant } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';
import { lastLink } from './support/mail';

const WRITE = { 'X-Mimorya-Request': '1' };
const TAB = 'Journal d’audit';
const CHOSEN = 'phrase-de-passe-journal-e2e';
/** The header row of the log's table. */
const LOG_HEADER = 'Date Acteur Action Résultat Détail';

/** What the test made, for its cleanup. */
const made: { paul?: Page; account?: string; roles: string[] } = { roles: [] };

test.afterEach(async () => {
  const paul = made.paul;
  if (!paul) return;
  try {
    if (made.account) {
      for (const assignment of await assignmentsOf(paul, made.account)) {
        const revoked = await paul.request.delete(`/api/admin/assignments/${assignment}`, {
          headers: WRITE,
        });
        expect.soft(revoked.status()).toBe(200);
      }
      const disabled = await paul.request.patch(`/api/admin/users/${made.account}`, {
        headers: WRITE,
        data: { status: 'disabled' } satisfies AccountUpdate,
      });
      expect.soft(disabled.status()).toBe(200);
    }
  } finally {
    for (const role of made.roles) {
      const removed = await paul.request.delete(`/api/admin/roles/${role}`, { headers: WRITE });
      expect.soft(removed.status()).toBe(200);
    }
    made.paul = made.account = undefined;
    made.roles = [];
  }
});

/** The identifiers of the assignments `account` holds, as the directory lists them. */
async function assignmentsOf(paul: Page, account: string): Promise<string[]> {
  const directory = (await (await paul.request.get('/api/admin/users')).json()) as Directory;
  const user = directory.users.find((u) => u.id === account);
  return (user?.assignments ?? []).map((assignment) => assignment.id);
}

/** A role of the test holding only `permissions`. */
async function role(paul: Page, name: string, permissions: string[]): Promise<string> {
  const created = await paul.request.post('/api/admin/roles', {
    headers: WRITE,
    data: { name, description: 'Rôle du test du journal d’audit', permissions } satisfies NewRole,
  });
  expect(created.status()).toBe(201);
  const id = ((await created.json()) as { id: string }).id;
  made.roles.push(id);
  return id;
}

/** Gives `account` the role `id` on the whole company. */
async function grant(paul: Page, account: string, id: string) {
  const granted = await paul.request.post(`/api/admin/users/${account}/assignments`, {
    headers: WRITE,
    data: { roleId: id, scope: { type: 'tenant' } } satisfies RoleGrant,
  });
  expect(granted.status()).toBe(201);
}

/** Opens the tab afresh as `page`, its lists loaded. */
async function openTab(page: Page) {
  await page.goto('/admin');
  await page.getByRole('tab', { name: TAB }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
}

test('each holder sees their part of the audit tab: the export alone, or the log alone', async ({
  page,
  newContext,
}) => {
  await openAdmin(page, TAB);
  made.paul = page;
  const stamp = Date.now();
  const exporting = await role(page, `Export du journal ${stamp} (e2e)`, ['audit:export']);
  const reading = await role(page, `Lecture du journal ${stamp} (e2e)`, ['audit:read']);
  const email = `journal.audit.${stamp}@neo-maintenance.example`;
  const account = await page.request.post('/api/admin/users', {
    headers: WRITE,
    data: { email, displayName: 'Inès Garnier' } satisfies NewAccount,
  });
  expect(account.status()).toBe(201);
  made.account = ((await account.json()) as { id: string }).id;
  await grant(page, made.account, exporting);

  const ines = await (await newContext()).newPage();
  const { link } = await lastLink(email, 'invitation');
  await ines.goto(link);
  await ines.getByLabel('Mot de passe (12 caractères minimum)').fill(CHOSEN);
  await ines.getByLabel('Confirmation').fill(CHOSEN);
  await ines.getByRole('button', { name: 'Activer mon compte' }).click();
  await ines.waitForURL('**/chat');

  // Exporting the log only: the tab opens on the tokens, not the log.
  await openTab(ines);
  await expect(ines.getByRole('heading', { name: 'Export vers un SIEM' })).toBeVisible();
  await expect(ines.getByRole('button', { name: 'Créer un jeton' })).toBeVisible();
  await expect(ines.getByRole('button', { name: 'Vérifier le journal' })).toHaveCount(0);
  await expect(ines.getByRole('row', { name: LOG_HEADER })).toHaveCount(0);
  await noError(ines);

  // Reading it only: the log and its verification, not the tokens.
  for (const assignment of await assignmentsOf(page, made.account)) {
    const revoked = await page.request.delete(`/api/admin/assignments/${assignment}`, {
      headers: WRITE,
    });
    expect(revoked.status()).toBe(200);
  }
  await grant(page, made.account, reading);
  await openTab(ines);
  await expect(ines.getByRole('button', { name: 'Vérifier le journal' })).toBeVisible();
  await expect(ines.getByRole('row', { name: LOG_HEADER })).toBeVisible();
  await expect(ines.getByRole('heading', { name: 'Export vers un SIEM' })).toHaveCount(0);
  await noError(ines);
});
