// A connector of the company that cannot start is named to the user whose
// request runs without it (ADR-048), and the run goes on with the others.
// The server the test adds is deleted, and a validation left waiting is
// refused, even when it fails.
import type { BrowserContext, Page } from '@playwright/test';
import type { ApprovalList, ApprovalVerdict, Connectors, NewConnector } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { REFERENCE_REQUEST } from './support/scenario';

const SERVER = 'Procédures e2e';
const WRITE = { 'X-Mimorya-Request': '1' };

/** The technical administrator, in a context of their own. */
async function administrator(newContext: () => Promise<BrowserContext>) {
  const page = await (await newContext()).newPage();
  await signIn(page, demo.paul);
  return page;
}

async function refuseWaiting(page: Page) {
  const approvals = await page.request.get('/api/approvals');
  if (approvals.status() !== 200) return;
  const { approvals: waiting } = (await approvals.json()) as ApprovalList;
  for (const approval of waiting.filter((a) => a.status === 'pending')) {
    const refused = await page.request.post(`/api/approvals/${approval.id}/decision`, {
      headers: WRITE,
      data: { approved: false } satisfies ApprovalVerdict,
    });
    expect(refused.status()).toBe(200);
  }
}

test.afterEach(async ({ page, newContext }) => {
  await refuseWaiting(page);
  const admin = await administrator(newContext);
  const listed = await admin.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.name === SERVER)) {
    const deleted = await admin.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

test('a request names the connector it goes without', async ({ page, newContext }) => {
  // A server on a private address: refused on this platform, it never starts.
  const admin = await administrator(newContext);
  const created = await admin.request.post('/api/admin/connectors', {
    headers: WRITE,
    data: { name: SERVER, prefix: 'proce', url: 'https://localhost/mcp' } satisfies NewConnector,
  });
  expect(created.status()).toBe(201);

  await signIn(page, demo.marc);
  await page.getByLabel('Votre message').fill(REFERENCE_REQUEST);
  await page.getByRole('button', { name: 'Envoyer' }).click();
  await expect(
    page.getByText(
      `Le connecteur « ${SERVER} » est indisponible : la réponse peut être incomplète.`,
    ),
  ).toBeVisible({ timeout: 30_000 });

  // The run goes on with the other connectors: it asks for its validation.
  await expect(page.getByText('Validation requise')).toBeVisible({ timeout: 30_000 });
  await page.getByRole('button', { name: 'Refuser' }).click();
  await expect(
    page.getByText('Le planning est inchangé : la validation a été refusée.'),
  ).toBeVisible({ timeout: 30_000 });
  await noError(page);
});
