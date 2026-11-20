// Deposited documents classified by the decision service (ADR-213, D4):
// Paul sets the documents' labels active, Jev the engine (the local
// TypeSafe of `e2e_server`, which answers the first type, « Contrat », and
// the lowest level, « Public »); Julien deposits two notes in his team's
// space at « Interne », finds each one's proposal in « Documents », applies
// the first, which his reload shows typed and public, and dismisses the
// second, left as deposited. Whatever happens, the notes are deleted and
// the settings removed. The rules themselves are tested on the API
// (`crates/api/tests/document_labels.rs`).
import { Buffer } from 'node:buffer';
import type { Page } from '@playwright/test';
import type {
  DecisionSettingsUpdate,
  DecisionUsageKey,
  DocumentItem,
  DocumentList,
} from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const DECISIONS = '/api/admin/decisions';
/** The only key the local TypeSafe opens to (`JEV_KEY`). */
const JEV_KEY = 'ts-cle-simulee-jev-0123456789abcdef';
const APPLIED = 'Note classée appliquée';
const DISMISSED = 'Note classée ignorée';
const USAGES: DecisionUsageKey[] = [
  'modelRouting',
  'skillChoice',
  'memoryGuard',
  'documentLabels',
  'actionGuard',
  'injectionDetection',
  'mailTriage',
];

/** Who changed what, for the cleanup. */
const left: { paul?: Page; julien?: Page } = {};

test.afterEach(async () => {
  const { paul, julien } = left;
  if (julien) {
    const list = (await (await julien.request.get('/api/documents')).json()) as DocumentList;
    for (const document of list.documents.filter((d) => [APPLIED, DISMISSED].includes(d.title))) {
      const deleted = await julien.request.delete(`/api/documents/${document.id}`, {
        headers: WRITE,
      });
      expect(deleted.status()).toBe(200);
    }
  }
  if (paul) {
    // 404: nothing to remove.
    const reset = await paul.request.delete(DECISIONS, { headers: WRITE });
    expect([200, 404]).toContain(reset.status());
  }
  left.paul = undefined;
  left.julien = undefined;
});

/** Deposits a note titled `title` into Julien's team's space, at « Interne ». */
async function deposit(julien: Page, title: string): Promise<string> {
  const list = (await (await julien.request.get('/api/documents')).json()) as DocumentList;
  const unitId = list.spaces[0]?.unitId;
  expect(unitId).toBeTruthy();
  const query = new URLSearchParams({
    name: `${title}.md`,
    unitId: unitId ?? '',
    sensitivity: 'internal',
  });
  const taken = await julien.request.post(`/api/documents?${query.toString()}`, {
    headers: { ...WRITE, 'Content-Type': 'text/markdown' },
    data: Buffer.from(`# ${title}\n\nContrat de maintenance du dépôt Nord.\n`),
  });
  expect(taken.status()).toBe(201);
  return ((await taken.json()) as { id: string }).id;
}

async function read(julien: Page, id: string): Promise<DocumentItem> {
  const response = await julien.request.get(`/api/documents/${id}`);
  expect(response.status()).toBe(200);
  return (await response.json()) as DocumentItem;
}

test('a team lead applies one classification and dismisses another', async ({ newContext }) => {
  const paul = await (await newContext()).newPage();
  await signIn(paul, demo.paul);
  left.paul = paul;
  const settings = {
    engine: 'jev',
    jevApiKey: JEV_KEY,
    usages: USAGES.map((key) => ({
      key,
      mode: key === 'documentLabels' ? 'active' : 'off',
      threshold: 0.5,
    })),
  } satisfies DecisionSettingsUpdate;
  const set = await paul.request.put(DECISIONS, { headers: WRITE, data: settings });
  expect(set.status()).toBe(200);

  const julien = await (await newContext()).newPage();
  await signIn(julien, demo.julien);
  left.julien = julien;
  const applied = await deposit(julien, APPLIED);
  const dismissed = await deposit(julien, DISMISSED);
  // Classified in the background, at once.
  for (const id of [applied, dismissed]) {
    await expect.poll(async () => (await read(julien, id)).labelProposal).not.toBeNull();
  }

  await julien.goto('/documents');
  await expect(julien.getByRole('heading', { name: 'Documents' })).toBeVisible();
  const said = 'Proposé par le service de décision : type « Contrat », confidentialité « Public ».';
  const first = julien.getByRole('group', { name: `Classement de « ${APPLIED} »` });
  await expect(first).toContainText(said);
  await first.getByRole('button', { name: 'Appliquer' }).click();
  await expect(first).toHaveCount(0);
  await julien.reload();
  const row = julien.getByRole('row').filter({ hasText: APPLIED });
  await expect(row.getByText('Contrat', { exact: true })).toBeVisible();
  await expect(row.getByText('Public', { exact: true })).toBeVisible();
  expect(await read(julien, applied)).toMatchObject({
    documentType: 'contract',
    sensitivity: 'public',
    labelProposal: null,
  });

  const second = julien.getByRole('group', { name: `Classement de « ${DISMISSED} »` });
  await expect(second).toContainText(said);
  await second.getByRole('button', { name: 'Ignorer' }).click();
  await expect(second).toHaveCount(0);
  await julien.reload();
  await expect(julien.getByRole('row').filter({ hasText: DISMISSED })).toBeVisible();
  await expect(second).toHaveCount(0);
  expect(await read(julien, dismissed)).toMatchObject({
    documentType: null,
    sensitivity: 'internal',
    labelProposal: null,
  });
  await noError(julien);
});
