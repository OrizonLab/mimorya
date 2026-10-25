// The « Documents » page (ADR-095, ADR-097): Julien deposits a note in his
// team's space; Lucas, of his team, reads and downloads it but neither
// deposits nor deletes; Julien deletes it. A file the API refuses says
// why, one beyond 10 MB is not sent, and the web relay carries a file of
// that bound whole. Lucas asks the agent, which answers from the company's charter
// and cites it, its file downloaded from the conversation (ADR-096).
// Whatever happens, the note is deleted.
import { Buffer } from 'node:buffer';
import type { Page } from '@playwright/test';
import type { DocumentList } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const NOTE = 'Consignes Nord';
const NOTE_FILE = `${NOTE}.md`;
const NOTE_TEXT = '# Consignes\n\nLe chauffe-eau du dépôt Nord se purge chaque lundi.\n';
const CHARTER = 'Charte qualité des interventions';
/** The question and answer the server's model plays (`e2e_server/documents.rs`). */
const ASKED = 'Sous quel délai une panne de chauffage est-elle prise en charge ?';
const ANSWERED = /une panne de chauffage signalée avant 12 h est prise en charge le jour même/;

/** Who deposited, for the cleanup. */
const left: { julien?: Page } = {};

/** The documents `page`'s user reads, by title. */
async function listed(page: Page) {
  const list = await page.request.get('/api/documents');
  expect(list.status()).toBe(200);
  return ((await list.json()) as DocumentList).documents;
}

test.afterEach(async () => {
  const { julien } = left;
  if (julien) {
    for (const document of (await listed(julien)).filter((d) => d.title === NOTE)) {
      const deleted = await julien.request.delete(`/api/documents/${document.id}`, {
        headers: WRITE,
      });
      expect(deleted.status()).toBe(200);
    }
  }
  left.julien = undefined;
});

test('a team lead deposits a note his team reads, then deletes it', async ({ newContext }) => {
  const julien = await (await newContext()).newPage();
  julien.on('dialog', (dialog) => dialog.accept());
  await signIn(julien, demo.julien);
  left.julien = julien;
  await julien.getByRole('link', { name: 'Documents' }).click();
  await expect(julien.getByRole('heading', { name: 'Documents' })).toBeVisible();
  // The company's charter, which the demo deposited.
  await expect(julien.getByText(CHARTER, { exact: true })).toBeVisible();

  // Julien deposits into his team's space only.
  const deposit = julien.getByRole('region', { name: 'Déposer un document' });
  await expect(deposit.getByLabel('Espace')).toHaveText('Équipe Nord');
  await deposit.getByLabel('Fichier').setInputFiles({
    name: NOTE_FILE,
    mimeType: 'text/markdown',
    buffer: Buffer.from(NOTE_TEXT),
  });
  await deposit.getByRole('button', { name: 'Déposer' }).click();
  await expect(deposit.getByRole('status')).toHaveText(
    `« ${NOTE_FILE} » est déposé : 1 passage(s) pour l’agent.`,
  );
  await julien.reload();
  const row = julien.getByRole('row').filter({ hasText: NOTE });
  await expect(row.getByText('Équipe Nord')).toBeVisible();
  await expect(row.getByText('Markdown')).toBeVisible();
  await expect(row.getByText('Interne')).toBeVisible();
  const recorded = (await listed(julien)).find((d) => d.title === NOTE);
  expect(recorded).toMatchObject({ fileName: NOTE_FILE, orgUnit: 'Équipe Nord', canDelete: true });

  // Lucas reads and downloads it, as deposited; he deposits and deletes nothing.
  const lucas = await (await newContext()).newPage();
  await signIn(lucas, demo.lucas);
  await lucas.goto('/documents');
  const seen = lucas.getByRole('row').filter({ hasText: NOTE });
  await expect(seen).toBeVisible();
  await expect(lucas.getByRole('region', { name: 'Déposer un document' })).toHaveCount(0);
  await expect(seen.getByRole('button', { name: `Supprimer « ${NOTE} »` })).toHaveCount(0);
  const link = seen.getByRole('link', { name: `Télécharger « ${NOTE} »` });
  const file = await lucas.request.get((await link.getAttribute('href')) ?? '');
  expect(file.status()).toBe(200);
  expect(file.headers()['content-disposition']).toContain('attachment');
  expect((await file.body()).toString()).toBe(NOTE_TEXT);
  await noError(lucas);

  // Julien deletes it: gone for both.
  await row.getByRole('button', { name: `Supprimer « ${NOTE} »` }).click();
  await expect(row).toHaveCount(0);
  await julien.reload();
  await expect(julien.getByRole('row').filter({ hasText: NOTE })).toHaveCount(0);
  expect((await listed(lucas)).some((d) => d.title === NOTE)).toBe(false);
  await noError(julien);
});

test('a refused file says why, and one beyond 10 MB is not sent', async ({ page }) => {
  await signIn(page, demo.julien);
  await page.goto('/documents');
  const deposit = page.getByRole('region', { name: 'Déposer un document' });
  await deposit.getByLabel('Fichier').setInputFiles({
    name: 'Ancien.doc',
    mimeType: 'application/msword',
    buffer: Buffer.from('Ancien format'),
  });
  await deposit.getByRole('button', { name: 'Déposer' }).click();
  await expect(deposit.getByRole('alert')).toContainText('enregistrez-le en .docx');

  let sent = false;
  page.on('request', (request) => {
    if (request.method() === 'POST' && request.url().includes('/api/documents')) sent = true;
  });
  await deposit.getByLabel('Fichier').setInputFiles({
    name: 'Gros.txt',
    mimeType: 'text/plain',
    buffer: Buffer.alloc(10 * 1024 * 1024 + 1, 'a'),
  });
  await deposit.getByRole('button', { name: 'Déposer' }).click();
  await expect(deposit.getByRole('alert')).toHaveText(
    'Ce fichier dépasse 10 Mo : il n’est pas envoyé.',
  );
  expect(sent).toBe(false);
  expect((await listed(page)).some((d) => d.fileName === 'Gros.txt')).toBe(false);
});

test('the web relay carries a file of the largest size the API takes', async ({ page }) => {
  await signIn(page, demo.julien);
  const list = await page.request.get('/api/documents');
  const [team] = ((await list.json()) as DocumentList).spaces;
  // 10 MiB: read by the API whole, too much text for a document, it says
  // so. A relay that cut the body short, as Next's does by default at
  // 10 MB, would give another answer: this guards its limit
  // (`next.config.ts`) against being lowered. Beyond, the page sends
  // nothing (above); the API's own refusal is tested on the API
  // (`crates/api/tests/document_uploads.rs`).
  const largest = await page.request.post(
    `/api/documents?name=Limite.txt&unitId=${team?.unitId ?? ''}`,
    {
      headers: { ...WRITE, 'Content-Type': 'application/octet-stream' },
      data: Buffer.alloc(10 * 1024 * 1024, 'a'),
    },
  );
  expect(largest.status()).toBe(422);
  expect((await largest.json()).error.message).toContain('Le texte du document dépasse');
});

test('the agent answers from the charter and cites it', async ({ page }) => {
  await signIn(page, demo.lucas);
  await page.getByLabel('Votre message').fill(ASKED);
  await page.getByRole('button', { name: 'Envoyer' }).click();
  await expect(page.getByText(ANSWERED)).toBeVisible({ timeout: 30_000 });
  await page.getByText(/source\(s\) consultée\(s\)/).click();
  const cited = page.getByRole('link', { name: `Télécharger le document « ${CHARTER} »` });
  await expect(cited).toBeVisible();
  const file = await page.request.get((await cited.getAttribute('href')) ?? '');
  expect(file.status()).toBe(200);
  expect((await file.body()).toString()).toContain('# Charte qualité des interventions');
  await noError(page);
});
