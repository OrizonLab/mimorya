// Who opens the « Journal d’audit » tab, and what they see there (ADR-182).
// An account made for the test, holding only the export of the log
// (`audit:export`, company-wide), opens the tab and sees the SIEM's tokens
// alone, neither the log nor its verification. Given the reading of the log
// instead (`audit:read`), it sees the log and its verification, not the
// tokens. Whatever happens, the account keeps no role and is disabled, and
// the test's roles are deleted (`support/accounts.ts`).
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

const TAB = 'Journal d’audit';
const CHOSEN = 'phrase-de-passe-journal-e2e';
/** The header row of the log's table. */
const LOG_HEADER = 'Date Acteur Action Résultat Détail';

/** What the test made, for its cleanup. */
const made: MadeAccounts & { paul?: Page } = { roles: [] };

test.afterEach(async () => {
  const paul = made.paul;
  if (!paul) return;
  made.paul = undefined;
  await retire(paul, made);
});

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
  const description = 'Rôle du test du journal d’audit';
  const exporting = await makeRole(page, made, {
    name: `Export du journal ${stamp} (e2e)`,
    description,
    permissions: ['audit:export'],
  });
  const reading = await makeRole(page, made, {
    name: `Lecture du journal ${stamp} (e2e)`,
    description,
    permissions: ['audit:read'],
  });
  const email = `journal.audit.${stamp}@neo-maintenance.example`;
  const account = await makeAccount(page, made, { email, displayName: 'Inès Garnier' });
  await grantCompanyWide(page, account, exporting);

  const ines = await (await newContext()).newPage();
  await activate(ines, email, CHOSEN);

  // Exporting the log only: the tab opens on the tokens, not the log.
  await openTab(ines);
  await expect(ines.getByRole('heading', { name: 'Export vers un SIEM' })).toBeVisible();
  await expect(ines.getByRole('button', { name: 'Créer un jeton' })).toBeVisible();
  await expect(ines.getByRole('button', { name: 'Vérifier le journal' })).toHaveCount(0);
  await expect(ines.getByRole('row', { name: LOG_HEADER })).toHaveCount(0);
  await noError(ines);

  // Reading it only: the log and its verification, not the tokens.
  await revokeAssignments(page, account);
  await grantCompanyWide(page, account, reading);
  await openTab(ines);
  await expect(ines.getByRole('button', { name: 'Vérifier le journal' })).toBeVisible();
  await expect(ines.getByRole('row', { name: LOG_HEADER })).toBeVisible();
  await expect(ines.getByRole('heading', { name: 'Export vers un SIEM' })).toHaveCount(0);
  await noError(ines);
});
