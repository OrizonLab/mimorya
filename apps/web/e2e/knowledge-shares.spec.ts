// A unit's knowledge shared with another (ADR-143, ADR-144): from the administration,
// Paul shares the north team's memory with the south team. Léa, of the
// south team, then reads the north team's memory, and no more once the
// share is removed. A share Paul would read through himself is refused, its
// reason shown. Whatever the test shares is removed whatever happens
// (`afterEach`). The rules themselves are tested on the API
// (`crates/api/tests/knowledge_share*.rs`).
import type { Page } from '@playwright/test';
import type { KnowledgeShares } from '../lib/api/contract';
import { cardWith, openAdmin, selectContaining } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const TAB = 'Partage de connaissances';
/** A memory of the north team's space (seed). */
const NORTH_MEMORY = 'Retours chantier équipe Nord';
const SHARE = 'Équipe Nord → Équipe Sud';

/** The administrator's page, for the cleanup. */
const left: { page?: Page } = {};

test.beforeEach(({ page }) => {
  // A removal asks for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async () => {
  const page = left.page;
  left.page = undefined;
  if (!page) return;
  // The demonstration company has none of its own.
  const listed = (await (
    await page.request.get('/api/admin/knowledge-shares')
  ).json()) as KnowledgeShares;
  for (const share of listed.shares) {
    const removed = await page.request.delete(`/api/admin/knowledge-shares/${share.id}`, {
      headers: WRITE,
    });
    expect(removed.status()).toBe(200);
  }
});

/** Whether whom `page` signed in reads the north team's memory. */
async function readsNorthMemory(page: Page, read: boolean) {
  await page.goto('/memory');
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  await expect(page.getByText(NORTH_MEMORY, { exact: true })).toHaveCount(read ? 1 : 0);
}

test('a team’s memory shared with another is read there until the share is removed', async ({
  page,
  newContext,
}) => {
  const lea = await (await newContext()).newPage();
  await signIn(lea, demo.lea);
  await readsNorthMemory(lea, false);

  left.page = page;
  await openAdmin(page, TAB);
  await expect(page.getByText('Aucun partage de connaissances')).toBeVisible();
  await page.getByRole('button', { name: 'Nouveau partage' }).click();
  await selectContaining(page.getByLabel('Connaissances de l’unité'), 'Équipe Nord');
  // Paul belongs to the administrative service: he would read it himself.
  await selectContaining(page.getByLabel('Partagées avec'), 'Service administratif');
  await page.getByRole('button', { name: 'Partager', exact: true }).click();
  await expect(page.getByText(/Vous seriez parmi les lecteurs de ce partage/)).toBeVisible();

  await selectContaining(page.getByLabel('Partagées avec'), 'Équipe Sud');
  await page.getByRole('checkbox', { name: 'Les documents qui y sont déposés' }).uncheck();
  await page.getByRole('button', { name: 'Partager', exact: true }).click();
  // The form closes once the share is made; then the share, as the API lists
  // it once the page is reloaded.
  await expect(page.getByRole('button', { name: 'Nouveau partage' })).toBeVisible();
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  const share = cardWith(page, SHARE, `Retirer ${SHARE}`);
  await expect(share.getByText('Mémoire', { exact: true })).toBeVisible();
  await expect(share.getByText('Documents', { exact: true })).toHaveCount(0);
  const said = 'Lu par les membres de l’unité « Équipe Sud », jusqu’au niveau « Interne » ;';
  await expect(page.getByText(`${said} sans échéance.`)).toBeVisible();
  await noError(page);
  await readsNorthMemory(lea, true);

  await page.getByRole('button', { name: `Retirer ${SHARE}` }).click();
  await expect(page.getByText('Aucun partage de connaissances')).toBeVisible();
  await readsNorthMemory(lea, false);
});
