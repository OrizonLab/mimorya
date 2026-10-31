// A long conversation whose earlier messages are summarised for the
// assistant (ADR-145, ADR-146): the end-to-end server leaves one of Marc's,
// nine exchanges of which the first six are folded into a summary
// (`e2e_server/summaries.rs`). The conversation says so after the last
// message the summary covers, and shows the summary on demand. Nothing is
// changed: the folding itself is tested on the API
// (`crates/api/tests/conversation_summar*.rs`).
import { demo, expect, noError, signIn, test } from './support/fixtures';

/** The conversation the server left (`e2e_server/summaries.rs`). */
const TITLE = 'Suivi du chantier Lumière';
const SUMMARY =
  'Marc suit le chantier Lumière : INT-2049 reste en cours, la mise en service est prévue lundi avec Léa.';
const NOTE = 'Les 12 messages ci-dessus sont résumés pour l’assistant';

test('the messages summarised for the assistant are said so, and the summary shown', async ({
  page,
}) => {
  await signIn(page, demo.marc);
  await page.getByRole('link', { name: TITLE }).click();

  const note = page.getByRole('note').filter({ hasText: NOTE });
  await expect(note).toBeVisible();
  // After the last message it covers, before the first it does not.
  const covered = page.getByText('Réponse 6 sur le chantier Lumière', { exact: true });
  const kept = page.getByText('Question 7 sur le chantier Lumière', { exact: true });
  const top = async (l: typeof note) => (await l.boundingBox())?.y ?? Number.NaN;
  const last = await top(covered);
  const at = await top(note);
  const first = await top(kept);
  expect(last).toBeLessThan(at);
  expect(at).toBeLessThan(first);

  // The summary itself, on demand.
  await expect(page.getByText(SUMMARY)).toBeHidden();
  await note.getByText('Voir le résumé').click();
  await expect(note.getByText(SUMMARY)).toBeVisible();
  await noError(page);
});
