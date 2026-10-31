// A new version of a document and a file dropped (ADR-150, ADR-151):
// Julien deposits a note by dropping its file on the deposit form, then
// replaces it by a new version from its row, whose panel takes the focus
// and gives it back: the same document, its new file downloaded, its
// version said; a file beyond 10 MB is not sent. A version replaced in
// between, as from another tab, is refused with its reason, nothing
// overwritten; the list reloaded, the panel says the version now there.
// Lucas, who reads the note, may not replace it. Whatever happens, the note
// is deleted.
import { Buffer } from 'node:buffer';
import type { Locator, Page, Request } from '@playwright/test';
import type { DocumentItem, DocumentList } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const NOTE = 'Consignes chaufferie';
const FIRST_FILE = `${NOTE}.md`;
const FIRST_TEXT = '# Consignes\n\nLa chaudière de la chaufferie se purge chaque lundi.\n';
const SECOND = `${NOTE} 2027`;
const SECOND_FILE = `${SECOND}.md`;
const SECOND_TEXT = '# Consignes\n\nLa chaudière se purge puis se rince chaque lundi.\n';
const THIRD_TEXT = '# Consignes\n\nLa chaudière se purge, se rince puis se contrôle.\n';

/** Who deposited, for the cleanup. */
const left: { julien?: Page } = {};

/** The documents `page`'s user reads. */
async function listed(page: Page): Promise<DocumentItem[]> {
  const list = await page.request.get('/api/documents');
  expect(list.status()).toBe(200);
  return ((await list.json()) as DocumentList).documents;
}

/** The note, whatever its version's title, as the API holds it. */
async function note(page: Page): Promise<DocumentItem | undefined> {
  return (await listed(page)).find((d) => d.title.startsWith(NOTE));
}

/** The file `link` downloads, as text. */
async function download(page: Page, link: Locator): Promise<string> {
  const file = await page.request.get((await link.getAttribute('href')) ?? '');
  expect(file.status()).toBe(200);
  return (await file.body()).toString();
}

/** Drops on `zone` the file `name` holding `text`, as a browser would. */
async function drop(zone: Locator, name: string, text: string) {
  const transfer = await zone.page().evaluateHandle(
    ({ name, text }) => {
      const dropped = new DataTransfer();
      dropped.items.add(new File([text], name, { type: 'text/markdown' }));
      return dropped;
    },
    { name, text },
  );
  await zone.dispatchEvent('dragover', { dataTransfer: transfer });
  await zone.dispatchEvent('drop', { dataTransfer: transfer });
}

test.afterEach(async () => {
  const { julien } = left;
  if (julien) {
    for (const document of (await listed(julien)).filter((d) => d.title.startsWith(NOTE))) {
      const deleted = await julien.request.delete(`/api/documents/${document.id}`, {
        headers: WRITE,
      });
      expect(deleted.status()).toBe(200);
    }
  }
  left.julien = undefined;
});

test('a dropped note is replaced by a new version, never over one made since', async ({
  newContext,
}) => {
  const julien = await (await newContext()).newPage();
  await signIn(julien, demo.julien);
  left.julien = julien;
  await julien.goto('/documents');

  // Julien drops the note's file on the deposit form.
  const deposit = julien.getByRole('region', { name: 'Déposer un document' });
  await drop(
    deposit.getByText('Choisissez un fichier, ou déposez-le ici.'),
    FIRST_FILE,
    FIRST_TEXT,
  );
  await expect(deposit.getByText(`Fichier retenu : ${FIRST_FILE}`)).toBeVisible();
  await deposit.getByRole('button', { name: 'Déposer' }).click();
  await expect(deposit.getByRole('status')).toHaveText(
    `« ${FIRST_FILE} » est déposé : 1 passage(s) pour l’agent.`,
  );
  const first = await note(julien);
  expect(first).toMatchObject({ fileName: FIRST_FILE, version: 1, replacedAt: null });

  // He opens its new version from its row: the panel takes the focus.
  await julien.reload();
  const open = julien.getByRole('button', { name: `Nouvelle version de « ${NOTE} »` });
  await open.click();
  await expect(open).toHaveAttribute('aria-expanded', 'true');
  const panel = julien.getByRole('region', { name: `Nouvelle version de « ${NOTE} »` });
  await expect(panel.getByRole('heading')).toBeFocused();
  await expect(panel).toContainText('Elle remplace la version 1');

  // A file beyond 10 MB is not sent. Cancelled, the focus is back on the row.
  let sent = false;
  const watch = (request: Request) => {
    if (request.method() === 'PUT' && request.url().includes('/api/documents')) sent = true;
  };
  julien.on('request', watch);
  await panel.getByLabel('Fichier').setInputFiles({
    name: 'Gros.txt',
    mimeType: 'text/plain',
    buffer: Buffer.alloc(10 * 1024 * 1024 + 1, 'a'),
  });
  await panel.getByRole('button', { name: 'Remplacer' }).click();
  await expect(panel.getByRole('alert')).toHaveText(
    'Ce fichier dépasse 10 Mo : il n’est pas envoyé.',
  );
  julien.off('request', watch);
  expect(sent).toBe(false);
  expect(await note(julien)).toMatchObject({ version: 1 });
  await panel.getByRole('button', { name: 'Annuler' }).click();
  await expect(panel).toHaveCount(0);
  await expect(open).toBeFocused();
  await expect(open).toHaveAttribute('aria-expanded', 'false');

  // He replaces it: the focus goes back to its row, renamed by its new title.
  await open.click();
  await panel.getByLabel('Fichier').setInputFiles({
    name: SECOND_FILE,
    mimeType: 'text/markdown',
    buffer: Buffer.from(SECOND_TEXT),
  });
  await panel.getByRole('button', { name: 'Remplacer' }).click();
  const library = julien.getByRole('region', { name: 'Documents', exact: true });
  await expect(library.getByRole('status')).toHaveText(
    `« ${NOTE} » est remplacé par « ${SECOND_FILE} » : version 2, 1 passage(s) pour l’agent.`,
  );
  await expect(panel).toHaveCount(0);
  const reopen = julien.getByRole('button', { name: `Nouvelle version de « ${SECOND} »` });
  await expect(reopen).toBeFocused();

  // The same document, its new file and its version, once reloaded.
  await julien.reload();
  const row = julien.getByRole('row').filter({ hasText: SECOND });
  await expect(row.getByText(/^Version 2, remplacée le /)).toBeVisible();
  const second = await note(julien);
  expect(second).toMatchObject({ id: first?.id, fileName: SECOND_FILE, version: 2 });
  expect(second?.createdAt).toBe(first?.createdAt);
  const link = row.getByRole('link', { name: `Télécharger « ${SECOND} »` });
  expect(await download(julien, link)).toBe(SECOND_TEXT);

  // Replaced in between, as from another tab: the page's attempt, made on
  // version 2, is refused with its reason; the list reloaded, the panel
  // says the version now there, which he then replaces knowingly.
  await reopen.click();
  const stale = julien.getByRole('region', { name: `Nouvelle version de « ${SECOND} »` });
  await expect(stale).toContainText('Elle remplace la version 2');
  const elsewhere = await julien.request.put(
    `/api/documents/${second?.id ?? ''}/file?name=${encodeURIComponent(SECOND_FILE)}&version=2`,
    {
      headers: { ...WRITE, 'Content-Type': 'application/octet-stream' },
      data: Buffer.from('# Consignes\n\nVersion d’un autre onglet.\n'),
    },
  );
  expect(elsewhere.status()).toBe(200);
  await stale.getByLabel('Fichier').setInputFiles({
    name: SECOND_FILE,
    mimeType: 'text/markdown',
    buffer: Buffer.from(THIRD_TEXT),
  });
  await stale.getByRole('button', { name: 'Remplacer' }).click();
  await expect(stale.getByRole('alert')).toHaveText(
    'Ce document a été remplacé entre-temps par une autre version : vérifiez-la avant de la remplacer',
  );
  await expect(stale).toContainText('Elle remplace la version 3');
  await expect(row.getByText(/^Version 3, remplacée le /)).toBeVisible();
  expect(await note(julien)).toMatchObject({ version: 3 });
  await stale.getByRole('button', { name: 'Remplacer' }).click();
  await expect(library.getByRole('status')).toHaveText(
    `« ${SECOND} » est remplacé par « ${SECOND_FILE} » : version 4, 1 passage(s) pour l’agent.`,
  );
  expect(await note(julien)).toMatchObject({ version: 4 });
  expect(await download(julien, link)).toBe(THIRD_TEXT);
  await noError(julien);

  // Lucas reads it, and may not replace it.
  const lucas = await (await newContext()).newPage();
  await signIn(lucas, demo.lucas);
  await lucas.goto('/documents');
  const seen = lucas.getByRole('row').filter({ hasText: SECOND });
  await expect(seen).toBeVisible();
  await expect(seen.getByRole('button', { name: /^Nouvelle version/ })).toHaveCount(0);
  await noError(lucas);
});
