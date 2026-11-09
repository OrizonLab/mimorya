// Each person downloads their own data from « Mon compte » (ADR-187,
// ADR-188): Marc's file holds his account and his conversations, never a
// password; the company's log keeps the trace of it. A refusal (another
// export of his under way) is said in French, and nothing is saved. What
// the file holds, and for whom alone, is tested on the API
// (`crates/api/tests/personal_export*.rs`).
import { readFile } from 'node:fs/promises';
import type { AuditLog, PersonalExport } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const EXPORTING = 'L’export de vos données est déjà en cours : attendez qu’il se termine.';

test('one downloads one’s own data', async ({ page, newContext }) => {
  await signIn(page, demo.marc);
  await page.goto('/account');
  const section = page.getByRole('region', { name: 'Mes données' });
  const downloading = page.waitForEvent('download');
  await section.getByRole('button', { name: 'Télécharger mes données' }).click();
  const download = await downloading;
  expect(download.suggestedFilename()).toMatch(/^mimorya-mes-donnees-\d{4}-\d{2}-\d{2}\.json$/);
  await expect(section.getByRole('status')).toHaveText('Vos données sont téléchargées.');

  const text = await readFile(await download.path(), 'utf8');
  const file = JSON.parse(text) as PersonalExport;
  expect(file.account.email).toBe(demo.marc);
  expect(file.account.units.map((unit) => unit.name)).toContain('Service technique');
  expect(Array.isArray(file.conversations)).toBe(true);
  expect(text).not.toContain('$argon2');
  await noError(page);

  // The company's log keeps it, read by its administrator.
  const paul = await (await newContext()).newPage();
  await signIn(paul, demo.paul);
  const log = await paul.request.get('/api/admin/audit?limit=50');
  expect(log.status()).toBe(200);
  const traced = ((await log.json()) as AuditLog).entries.filter(
    (entry) => entry.action === 'account.exported',
  );
  expect(traced[0]?.actorUserId).toBe(file.account.id);
  expect(traced[0]?.metadata).toMatchObject({
    conversations: file.conversations.length,
    runs: file.runs.length,
  });
});

test('a refused download is said and nothing is saved', async ({ page }) => {
  await signIn(page, demo.marc);
  await page.goto('/account');
  await page.route('**/api/account/export', (route) =>
    route.fulfill({ status: 409, json: { error: { code: 'conflict', message: EXPORTING } } }),
  );
  let saved = false;
  page.on('download', () => (saved = true));
  const section = page.getByRole('region', { name: 'Mes données' });
  await section.getByRole('button', { name: 'Télécharger mes données' }).click();
  await expect(section.getByText(EXPORTING)).toBeVisible();
  await expect(section.getByRole('status')).toHaveCount(0);
  await expect(section.getByRole('button', { name: 'Télécharger mes données' })).toBeEnabled();
  expect(saved).toBe(false);
});
