// A new version of a document and a file dropped (ADR-150, ADR-151):
// Julien deposits a note by dropping its file on the deposit form, then
// replaces it by a new version from its row: the same document, its new
// file downloaded, its version said. A version replaced in between, as from
// another tab, is refused with its reason, nothing overwritten. Lucas, who
// reads the note, may not replace it. Whatever happens, the note is deleted.
import { Buffer } from 'node:buffer';
import type { Locator, Page } from '@playwright/test';
import type { DocumentItem, DocumentList } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const NOTE = 'Consignes chaufferie';
const FIRST_FILE = `${NOTE}.md`;
const FIRST_TEXT = '# Consignes\n\nLa chaudière de la chaufferie se purge chaque lundi.\n';
const SECOND = `${NOTE} 2027`;
const SECOND_FILE = `${SECOND}.md`;
const SECOND_TEXT = '# Consignes\n\nLa chaudière se purge puis se rince chaque lundi.\n';

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

  // He replaces it by a new version from its row.
  await julien.reload();
  await julien.getByRole('button', { name: `Nouvelle version de « ${NOTE} »` }).click();
  const panel = julien.getByRole('region', { name: `Nouvelle version de « ${NOTE} »` });
  await expect(panel).toContainText('Elle remplace la version 1');
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

  // The same document, its new file and its version, once reloaded.
  await julien.reload();
  const row = julien.getByRole('row').filter({ hasText: SECOND });
  await expect(row.getByText(/^Version 2, remplacée le /)).toBeVisible();
  const second = await note(julien);
  expect(second).toMatchObject({ id: first?.id, fileName: SECOND_FILE, version: 2 });
  expect(second?.createdAt).toBe(first?.createdAt);
  const link = row.getByRole('link', { name: `Télécharger « ${SECOND} »` });
  const file = await julien.request.get((await link.getAttribute('href')) ?? '');
  expect((await file.body()).toString()).toBe(SECOND_TEXT);

  // Replaced in between, as from another tab: the page's attempt, made on
  // version 2, is refused with its reason, and version 3 stays.
  await row.getByRole('button', { name: `Nouvelle version de « ${SECOND} »` }).click();
  const stale = julien.getByRole('region', { name: `Nouvelle version de « ${SECOND} »` });
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
    buffer: Buffer.from('# Consignes\n\nVersion périmée.\n'),
  });
  await stale.getByRole('button', { name: 'Remplacer' }).click();
  await expect(stale.getByRole('alert')).toHaveText(
    'Ce document a été remplacé entre-temps : rechargez la liste pour voir sa dernière version',
  );
  expect(await note(julien)).toMatchObject({ version: 3 });
  await stale.getByRole('button', { name: 'Annuler' }).click();
  await expect(stale).toHaveCount(0);
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
