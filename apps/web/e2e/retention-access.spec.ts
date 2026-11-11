// Who opens the « Conservation des données » tab (ADR-196). An account made
// for the test, holding only `retention:manage` on the whole company, opens
// the tab and reads the company's durations there. Given `settings:manage`
// instead, it opens « Modèle d’IA », never the durations. The API refuses
// them to it as well (`crates/api/tests/retention_durations.rs`). Whatever
// happens, the account keeps no role and is disabled then erased, and the
// test's roles are deleted (`support/accounts.ts`).
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

const TAB = 'Conservation des données';
const CHOSEN = 'phrase-de-passe-conservation-e2e';

/** What the test made, for its cleanup. */
const made: MadeAccounts & { paul?: Page } = { roles: [] };

test.afterEach(async () => {
  const paul = made.paul;
  if (!paul) return;
  made.paul = undefined;
  await retire(paul, made);
});

test('the durations tab opens to who holds retention:manage, not settings:manage', async ({
  page,
  newContext,
}) => {
  await openAdmin(page, TAB);
  made.paul = page;
  const stamp = Date.now();
  const description = 'Rôle du test de la conservation des données';
  const retention = await makeRole(page, made, {
    name: `Conservation ${stamp} (e2e)`,
    description,
    permissions: ['retention:manage'],
  });
  const settings = await makeRole(page, made, {
    name: `Paramètres ${stamp} (e2e)`,
    description,
    permissions: ['settings:manage'],
  });
  const email = `conservation.${stamp}@neo-maintenance.example`;
  const account = await makeAccount(page, made, { email, displayName: 'Rose Lemaire' });
  await grantCompanyWide(page, account, retention);

  const rose = await (await newContext()).newPage();
  await activate(rose, email, CHOSEN);

  // `retention:manage` alone: the tab opens on the company's durations.
  await rose.goto('/admin');
  await rose.getByRole('tab', { name: TAB }).click();
  const form = rose.getByRole('form', { name: 'Durées de conservation' });
  await expect(form.getByLabel('Conversations inactives (mois)')).toHaveValue('24');
  await expect(rose.getByRole('tab', { name: 'Modèle d’IA' })).toHaveCount(0);
  await noError(rose);

  // `settings:manage` alone: the model's tab, never the durations'.
  await revokeAssignments(page, account);
  await grantCompanyWide(page, account, settings);
  await rose.goto('/admin');
  await expect(rose.getByRole('tab', { name: 'Modèle d’IA' })).toBeVisible();
  await expect(rose.getByRole('tab', { name: TAB })).toHaveCount(0);
  await expect(form).toHaveCount(0);
  await noError(rose);
});
