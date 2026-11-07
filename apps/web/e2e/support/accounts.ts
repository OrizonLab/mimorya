// Accounts a test makes for itself (CLAUDE.md, « Tests »): an address of its
// own, roles made for it and given on the whole company, the account
// activated from its invitation; then, whatever happened, its assignments
// revoked, the account disabled (the API deletes no account) and its roles
// deleted, so that it keeps no business role.
import type { Page } from '@playwright/test';
import type {
  AccountUpdate,
  Directory,
  NewAccount,
  NewRole,
  RoleGrant,
} from '../../lib/api/contract';
import { expect } from './fixtures';
import { lastLink } from './mail';

const WRITE = { 'X-Mimorya-Request': '1' };

/** What a test made of accounts and roles, for its cleanup (`retire`). */
export interface MadeAccounts {
  account?: string;
  roles: string[];
}

/** A role holding only `permissions`, made by `admin` and noted in `made`. */
export async function makeRole(admin: Page, made: MadeAccounts, role: NewRole): Promise<string> {
  const created = await admin.request.post('/api/admin/roles', { headers: WRITE, data: role });
  expect(created.status()).toBe(201);
  const id = ((await created.json()) as { id: string }).id;
  made.roles.push(id);
  return id;
}

/** An account invited by `admin` at its address, noted in `made`. */
export async function makeAccount(
  admin: Page,
  made: MadeAccounts,
  account: NewAccount,
): Promise<string> {
  const created = await admin.request.post('/api/admin/users', { headers: WRITE, data: account });
  expect(created.status()).toBe(201);
  made.account = ((await created.json()) as { id: string }).id;
  return made.account;
}

/** Gives `account` the role `role` on the whole company. */
export async function grantCompanyWide(admin: Page, account: string, role: string) {
  const granted = await admin.request.post(`/api/admin/users/${account}/assignments`, {
    headers: WRITE,
    data: { roleId: role, scope: { type: 'tenant' } } satisfies RoleGrant,
  });
  expect(granted.status()).toBe(201);
}

/** Activates on `page` the account invited at `email`, with `password`, from its invitation. */
export async function activate(page: Page, email: string, password: string) {
  const { link } = await lastLink(email, 'invitation');
  await page.goto(link);
  await page.getByLabel('Mot de passe (12 caractères minimum)').fill(password);
  await page.getByLabel('Confirmation').fill(password);
  await page.getByRole('button', { name: 'Activer mon compte' }).click();
  await page.waitForURL('**/chat');
}

/**
 * Revokes every assignment `account` holds, as the directory lists them;
 * `soft`, a refusal is noted without stopping (a cleanup).
 */
export async function revokeAssignments(admin: Page, account: string, soft = false) {
  const directory = (await (await admin.request.get('/api/admin/users')).json()) as Directory;
  const user = directory.users.find((u) => u.id === account);
  for (const assignment of user?.assignments ?? []) {
    const revoked = await admin.request.delete(`/api/admin/assignments/${assignment.id}`, {
      headers: WRITE,
    });
    (soft ? expect.soft : expect)(revoked.status()).toBe(200);
  }
}

/**
 * The cleanup of what `made` holds, whatever failed before: the account's
 * assignments revoked and the account disabled, then the roles deleted.
 * `made` is emptied.
 */
export async function retire(admin: Page, made: MadeAccounts) {
  try {
    if (made.account) {
      await revokeAssignments(admin, made.account, true);
      const disabled = await admin.request.patch(`/api/admin/users/${made.account}`, {
        headers: WRITE,
        data: { status: 'disabled' } satisfies AccountUpdate,
      });
      expect.soft(disabled.status()).toBe(200);
    }
  } finally {
    for (const role of made.roles) {
      const removed = await admin.request.delete(`/api/admin/roles/${role}`, { headers: WRITE });
      expect.soft(removed.status()).toBe(200);
    }
    made.account = undefined;
    made.roles = [];
  }
}
