// The SIEM export tokens of the « Journal d’audit » tab (ADR-182): Paul makes
// a token named for the SIEM. Its secret is shown this once, and reads the
// audit log at the address shown; the notice closed, the focus is on the
// section's title. Read again after reloading, the token is listed by its
// name, its last use noted, its secret gone, and the log traces its making.
// Removed after a confirmation, it reads nothing more. Who sees the tab and
// what: `audit-export-access.spec.ts`. The rules themselves are tested on the
// API (`crates/api/tests/audit_export*.rs`).
import type { AuditExport, AuditExportTokens } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const TAB = 'Journal d’audit';
const TOKENS = '/api/admin/audit/export-tokens';
const NAME = 'SIEM de la DSI (bout en bout)';

// What the test made, removed even when it fails; 401: it never signed in.
test.afterEach(async ({ page }) => {
  const listed = await page.request.get(TOKENS);
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { tokens } = (await listed.json()) as AuditExportTokens;
  for (const token of tokens.filter((t) => t.name === NAME)) {
    const removed = await page.request.delete(`${TOKENS}/${token.id}`, {
      headers: { 'X-Mimorya-Request': '1' },
    });
    expect(removed.status()).toBe(200);
  }
});

test('an administrator makes a SIEM export token, shown once, then removes it', async ({
  page,
}) => {
  await openAdmin(page, TAB);
  await expect(page.getByRole('heading', { name: 'Export vers un SIEM' })).toBeVisible();
  const address = page.getByText(/\/api\/audit\/export$/);
  await expect(address).toBeVisible();
  const exportUrl = (await address.innerText()).trim();
  expect(exportUrl).toBe('http://localhost:3000/api/audit/export');

  await page.getByLabel(/Nom du nouveau jeton/).fill(NAME);
  await page.getByRole('button', { name: 'Créer un jeton' }).click();
  const notice = page.getByRole('region', { name: `Jeton « ${NAME} »` });
  await expect(notice).toBeVisible();
  const secret = await notice.getByLabel(/Jeton, à envoyer/).inputValue();
  expect(secret).toMatch(/^mae_[A-Za-z0-9_-]{40,}$/);
  await noError(page);

  // The SIEM reads the sealed log with it, at the address shown, from the start.
  const read = (token: string) =>
    page.request.get(`${exportUrl}?limit=5`, {
      headers: { Authorization: `Bearer ${token}` },
    });
  const first = await read(secret);
  expect(first.status()).toBe(200);
  const page1 = (await first.json()) as AuditExport;
  const [opening] = page1.entries;
  expect(opening?.place).toBe(1);
  expect(opening?.seal).toMatch(/^[0-9a-f]{64}$/);

  // Noted, the notice goes; read again, the token is listed, used, its secret
  // shown nowhere, its making traced.
  await notice.getByRole('button', { name: 'J’ai noté le jeton' }).click();
  await expect(notice).toHaveCount(0);
  const title = page.getByRole('heading', { name: 'Export vers un SIEM' });
  await expect(title).toBeFocused();
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  const row = page.getByRole('row').filter({ hasText: NAME });
  await expect(row).toBeVisible();
  await expect(row.getByText('Jamais')).toHaveCount(0);
  await expect(page.getByText(secret)).toHaveCount(0);
  await expect(
    page.getByRole('row').filter({ hasText: 'audit.export_token_created' }).first(),
  ).toBeVisible();

  // Removed after a confirmation, it reads nothing more.
  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: `Retirer le jeton ${NAME}` }).click();
  await expect(row).toHaveCount(0);
  // Its button gone with its row, the focus is back on the section's title.
  await expect(title).toBeFocused();
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  await expect(page.getByRole('row').filter({ hasText: NAME })).toHaveCount(0);
  expect((await read(secret)).status()).toBe(401);
  await noError(page);
});
