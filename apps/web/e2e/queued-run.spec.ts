// A run waiting for a worker (ADR-085): the end-to-end server leaves one
// of Marc's queued, as an `api` process queues it, and holds it until his
// browser has followed it for a few seconds. Meanwhile the conversation
// says it waits in the queue, and no other request can be sent in it; then
// the server's queue claims it, and its answer comes in the same page,
// kept once the page is read again.
import { demo, expect, noError, signIn, test } from './support/fixtures';

/** The conversation the server left queued (`e2e_server/queued.rs`). */
const QUEUED = 'Demande en file';
const WAITING = 'En file : votre demande sera traitée dans un instant';
const ANSWER = 'Deux comptes rendus attendent encore une réponse.';

test('a queued request says it waits, then gets its answer', async ({ page }) => {
  await signIn(page, demo.marc);
  await page.getByRole('link', { name: QUEUED }).click();

  const composer = page.getByLabel('Votre message');
  await expect(page.getByText(WAITING)).toBeVisible();
  await expect(composer).toBeDisabled();
  await expect(composer).toHaveAttribute('placeholder', 'Traitement en cours…');

  // Claimed once followed for a while: the answer comes in, the queue is gone.
  await expect(page.getByText(ANSWER)).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(WAITING)).toHaveCount(0);
  await expect(composer).toBeEnabled();
  await page.reload();
  await expect(page.getByText(ANSWER)).toBeVisible();
  await expect(page.getByText(WAITING)).toHaveCount(0);
  await noError(page);
});
