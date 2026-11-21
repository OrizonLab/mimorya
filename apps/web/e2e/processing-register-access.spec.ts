// Who opens the « Registre des traitements » tab (ADR-218). An account made
// for the test, holding only `compliance:read` on the whole company (a data
// protection officer), opens the register and none of the administration's
// other screens. Given `retention:manage` instead, it opens « Conservation
// des données », never the register. The API refuses it to it as well
// (`crates/api/tests/processing_register_access.rs`). Whatever happens, the
// account keeps no role and is disabled then erased, and the test's roles are
// deleted (`support/accounts.ts`).
import type { Page } from '@playwright/test';
import {
  activate,
  grantCompanyWide,
  makeAccount,
  makeRole,
  retire,
  revokeAssignments,
  type MadeAccounts,
} from './support/accounts';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const TAB = 'Registre des traitements';
const CHOSEN = 'phrase-de-passe-registre-e2e';

/** What the test made, for its cleanup. */
const made: MadeAccounts & { paul?: Page } = { roles: [] };

test.afterEach(async () => {
  const paul = made.paul;
  if (!paul) return;
  made.paul = undefined;
  await retire(paul, made);
});

test('the register opens to who holds compliance:read, alone', async ({ page, newContext }) => {
  await openAdmin(page, TAB);
  made.paul = page;
  const stamp = Date.now();
  const description = 'Rôle du test du registre des traitements';
  const officer = await makeRole(page, made, {
    name: `Registre ${stamp} (e2e)`,
    description,
    permissions: ['compliance:read'],
  });
  const retention = await makeRole(page, made, {
    name: `Conservation ${stamp} (e2e)`,
    description,
    permissions: ['retention:manage'],
  });
  const email = `registre.${stamp}@neo-maintenance.example`;
  const account = await makeAccount(page, made, { email, displayName: 'Alice Moreau' });
  await grantCompanyWide(page, account, officer);

  const alice = await (await newContext()).newPage();
  await activate(alice, email, CHOSEN);

  // `compliance:read` alone: the register, and none of the administration's
  // other screens (the base role reads the organisation and its people).
  await alice.goto('/admin');
  await expect(alice.getByRole('tab', { name: TAB })).toBeVisible();
  for (const other of [
    'Modèle d’IA',
    'Conservation des données',
    'Journal d’audit',
    'Connecteurs',
  ]) {
    await expect(alice.getByRole('tab', { name: other })).toHaveCount(0);
  }
  await alice.getByRole('tab', { name: TAB }).click();
  await expect(alice.getByRole('region', { name: TAB })).toContainText('Néo Maintenance (démo)');
  await noError(alice);

  // `retention:manage` alone: the durations' tab, never the register.
  await revokeAssignments(page, account);
  await grantCompanyWide(page, account, retention);
  await alice.goto('/admin');
  await expect(alice.getByRole('tab', { name: 'Conservation des données' })).toBeVisible();
  await expect(alice.getByRole('tab', { name: TAB })).toHaveCount(0);
  await expect(alice.getByRole('region', { name: TAB })).toHaveCount(0);
  await noError(alice);
});
