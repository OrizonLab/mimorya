// The SIEM export tokens of the « Journal d’audit » tab (ADR-182):
// - Paul makes a token named for the SIEM. Its secret is shown this once,
//   copied (each copy said afresh), and reads the audit log at the address
//   shown; the notice closed, the focus is on the section's title. Read again
//   after reloading, the token is listed by its name, its last use noted, its
//   secret gone, and the log traces its making. Removed after a
//   confirmation, it reads nothing more;
// - with a list another administrator changed meanwhile (here, the API), a
//   making refused at the limit and a removal of a token already gone each
//   leave the focus on the title, not the page; at the limit, the making is
//   no longer offered, and below it again, it is.
// Every token a test makes carries `MARK`, removed even when it fails. Who
// sees the tab and what: `audit-export-access.spec.ts`. The rules themselves
// are tested on the API (`crates/api/tests/audit_export*.rs`).
import type { Page } from '@playwright/test';
import type {
  AuditExport,
  BearerTokenCreated,
  AuditExportTokens,
  NewBearerToken,
} from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const TAB = 'Journal d’audit';
const TOKENS = '/api/admin/audit/export-tokens';
const WRITE = { 'X-Mimorya-Request': '1' };
const MARK = '(bout en bout)';
const NAME = `SIEM de la DSI ${MARK}`;
const COPIED = 'Jeton copié dans le presse-papiers.';

// What the tests made, removed even when they fail; 401: never signed in.
test.afterEach(async ({ page }) => {
  const listed = await page.request.get(TOKENS);
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { tokens } = (await listed.json()) as AuditExportTokens;
  for (const token of tokens.filter((t) => t.name.endsWith(MARK))) {
    const removed = await page.request.delete(`${TOKENS}/${token.id}`, { headers: WRITE });
    expect(removed.status()).toBe(200);
  }
});

/** Opens the tab afresh, its lists loaded. */
async function openTab(page: Page) {
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
}

/** A token made by the API, as another administrator would. */
async function makeElsewhere(page: Page, name: string): Promise<string> {
  const made = await page.request.post(TOKENS, {
    headers: WRITE,
    data: { name } satisfies NewBearerToken,
  });
  expect(made.status()).toBe(201);
  return ((await made.json()) as BearerTokenCreated).token.id;
}

test('an administrator makes a SIEM export token, shown once, then removes it', async ({
  page,
}) => {
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
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

  // Copied twice: the second copy's words are new ones, said again.
  const copy = notice.getByRole('button', { name: 'Copier le jeton' });
  const status = notice.getByRole('status');
  await copy.click();
  await expect(status).toHaveText(COPIED);
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(secret);
  const said = await status.locator('span').elementHandle();
  await copy.click();
  await expect.poll(() => said?.evaluate((words) => words.isConnected)).toBe(false);
  await expect(status).toHaveText(COPIED);

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
  await openTab(page);
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
  await openTab(page);
  await expect(page.getByRole('row').filter({ hasText: NAME })).toHaveCount(0);
  expect((await read(secret)).status()).toBe(401);
  await noError(page);
});

test('attempts refused by a list changed elsewhere keep the focus on the title; the limit stops the making', async ({
  page,
}) => {
  await openAdmin(page, TAB);
  const title = page.getByRole('heading', { name: 'Export vers un SIEM' });
  const making = page.getByRole('button', { name: 'Créer un jeton' });
  const first = `SIEM 1 ${MARK}`;
  const firstId = await makeElsewhere(page, first);
  for (const n of [2, 3, 4]) await makeElsewhere(page, `SIEM ${n} ${MARK}`);
  await openTab(page);
  await expect(page.getByRole('row').filter({ hasText: MARK })).toHaveCount(4);

  // The fifth made elsewhere while this list shows four: making a sixth here
  // is refused, and the form, which had the focus, gives way to the limit.
  await makeElsewhere(page, `SIEM 5 ${MARK}`);
  await page.getByLabel(/Nom du nouveau jeton/).fill(`SIEM 6 ${MARK}`);
  await making.click();
  await expect(page.getByText(/^L’entreprise a déjà 5 jetons d’export/)).toBeVisible();
  await expect(
    page.getByText('L’entreprise a déjà 5 jetons : retirez-en un pour en créer un autre.'),
  ).toBeVisible();
  await expect(making).toHaveCount(0);
  await expect(page.getByRole('row').filter({ hasText: MARK })).toHaveCount(5);
  await expect(title).toBeFocused();

  // The first removed elsewhere: removing it here is refused, its row goes
  // with the list read again, and the focus is back on the title.
  const removed = await page.request.delete(`${TOKENS}/${firstId}`, { headers: WRITE });
  expect(removed.status()).toBe(200);
  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: `Retirer le jeton ${first}` }).click();
  await expect(page.getByText('Jeton d’export introuvable')).toBeVisible();
  await expect(page.getByRole('row').filter({ hasText: first })).toHaveCount(0);
  await expect(title).toBeFocused();

  // Below the limit again, the making is offered.
  await expect(making).toBeVisible();
});
