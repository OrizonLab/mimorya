// The new-version panel of a document (ADR-151): its row's button opens
// and closes it. It keeps the version it opened on: the list reloaded by
// another action shows a newer one, which the panel says, and its
// replacement is refused, then it says the version now there. While it
// sends, the rows' actions wait and a file dropped is not taken. Its
// document gone from the reloaded list, it closes. Whatever happens, the
// notes are deleted.
import { Buffer } from 'node:buffer';
import type { Page } from '@playwright/test';
import type { DocumentList } from '../lib/api/contract';
import { drop } from './support/drop';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const NOTE = 'Consignes panneau';
const A = `${NOTE} A`;
const B = `${NOTE} B`;
const C = `${NOTE} C`;

/** Who deposited, for the cleanup. */
const left: { julien?: Page } = {};

async function list(page: Page): Promise<DocumentList> {
  const listed = await page.request.get('/api/documents');
  expect(listed.status()).toBe(200);
  return (await listed.json()) as DocumentList;
}

/** Deposits through the API, into the user's team, the note `title`: its identifier. */
async function deposited(page: Page, title: string): Promise<string> {
  const [team] = (await list(page)).spaces;
  const created = await page.request.post(
    `/api/documents?name=${encodeURIComponent(`${title}.md`)}&unitId=${team?.unitId ?? ''}`,
    {
      headers: { ...WRITE, 'Content-Type': 'application/octet-stream' },
      data: Buffer.from(`# ${title}\n\nPurger le circuit.\n`),
    },
  );
  expect(created.status()).toBe(201);
  return ((await created.json()) as { id: string }).id;
}

test.afterEach(async () => {
  const { julien } = left;
  if (julien) {
    for (const document of (await list(julien)).documents) {
      if (!document.title.startsWith(NOTE)) continue;
      const deleted = await julien.request.delete(`/api/documents/${document.id}`, {
        headers: WRITE,
      });
      expect(deleted.status()).toBe(200);
    }
  }
  left.julien = undefined;
});

test('the panel keeps the version it opened on and closes with its document', async ({ page }) => {
  page.on('dialog', (dialog) => dialog.accept());
  await signIn(page, demo.julien);
  left.julien = page;
  const a = await deposited(page, A);
  await deposited(page, B);
  await page.goto('/documents');

  // Its row's button opens and closes the panel.
  const open = page.getByRole('button', { name: `Nouvelle version de « ${A} »` });
  const panel = page.getByRole('region', { name: `Nouvelle version de « ${A} »` });
  await open.click();
  await expect(panel).toContainText('Elle remplace la version 1');
  await open.click();
  await expect(panel).toHaveCount(0);
  await expect(open).toHaveAttribute('aria-expanded', 'false');
  await open.click();
  await expect(panel).toContainText('Elle remplace la version 1');

  // Replaced elsewhere, then the list reloaded by another action: the
  // panel keeps version 1, and says the list shows version 2.
  const elsewhere = await page.request.put(
    `/api/documents/${a}/file?name=${encodeURIComponent(`${A}.md`)}&version=1`,
    {
      headers: { ...WRITE, 'Content-Type': 'application/octet-stream' },
      data: Buffer.from(`# ${A}\n\nVersion d’un autre onglet.\n`),
    },
  );
  expect(elsewhere.status()).toBe(200);
  await page.getByRole('button', { name: `Supprimer « ${B} »` }).click();
  await expect(page.getByRole('row').filter({ hasText: B })).toHaveCount(0);
  const rowA = page.getByRole('row').filter({ hasText: A });
  await expect(rowA.getByText(/^Version 2, remplacée le /)).toBeVisible();
  await expect(panel).toContainText('Elle remplace la version 1');
  await expect(panel).toContainText(
    'La liste montre maintenant la version 2 : la version 1 n’est plus la dernière, et la remplacer sera refusé.',
  );

  // While it sends, held here, the rows' actions wait and a file dropped
  // is not taken.
  let release = () => {};
  const held = new Promise<void>((done) => (release = done));
  const replacing = (url: URL) => url.pathname === `/api/documents/${a}/file`;
  await page.route(replacing, async (route) => {
    if (route.request().method() === 'PUT') await held;
    await route.continue();
  });
  await panel.getByLabel('Fichier').setInputFiles({
    name: `${A}.md`,
    mimeType: 'text/markdown',
    buffer: Buffer.from(`# ${A}\n\nVersion de la page.\n`),
  });
  const sent = page.waitForRequest((r) => r.method() === 'PUT' && replacing(new URL(r.url())));
  await panel.getByRole('button', { name: 'Remplacer' }).click();
  await sent;
  await expect(open).toBeDisabled();
  await expect(rowA.getByRole('button', { name: `Supprimer « ${A} »` })).toBeDisabled();
  await drop(panel.getByText(`Fichier retenu : ${A}.md`), 'Autre.md', '# Autre\n');
  await expect(panel.getByText(`Fichier retenu : ${A}.md`)).toBeVisible();
  release();

  // Refused for version 1, it says the version now there.
  await expect(panel.getByRole('alert')).toHaveText(
    'Ce document a été remplacé entre-temps par une autre version : vérifiez-la avant de la remplacer',
  );
  await expect(panel).toContainText('Elle remplace la version 2');
  await expect(panel).not.toContainText('La liste montre maintenant');
  await page.unroute(replacing);
  const kept = (await list(page)).documents.find((d) => d.id === a);
  expect(kept).toMatchObject({ version: 2 });
  await expect(open).toBeEnabled();

  // Gone, deleted elsewhere: the list reloaded by a deposit, the panel
  // closes with it.
  const deleted = await page.request.delete(`/api/documents/${a}`, { headers: WRITE });
  expect(deleted.status()).toBe(200);
  const deposit = page.getByRole('region', { name: 'Déposer un document' });
  await deposit.getByLabel('Fichier').setInputFiles({
    name: `${C}.md`,
    mimeType: 'text/markdown',
    buffer: Buffer.from(`# ${C}\n\nPurger le circuit.\n`),
  });
  await deposit.getByRole('button', { name: 'Déposer' }).click();
  await expect(page.getByRole('row').filter({ hasText: C })).toBeVisible();
  await expect(panel).toHaveCount(0);
  await expect(page.getByRole('row').filter({ hasText: A })).toHaveCount(0);
  await noError(page);
});
