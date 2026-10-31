// Long conversations whose earlier messages are summarised for the
// assistant (ADR-145, ADR-146): the end-to-end server leaves four of
// Marc's (`e2e_server/summaries.rs`). Each says so where its summarised
// messages stop: after the last one when they are all shown above; first
// when none is shown any more (a conversation shows its last 200 messages);
// after the last one, counting those no longer shown, when only some are.
// A summary that lags says how many messages after it are neither
// summarised nor reread yet (ADR-149). The summary is shown on demand.
// Nothing is changed: the folding itself is tested on the API
// (`crates/api/tests/conversation_summar*.rs`).
import type { Locator, Page } from '@playwright/test';
import { demo, expect, noError, signIn, test } from './support/fixtures';

/** One of the messages the server wrote on `site`'s conversation. */
function message(page: Page, text: string, site: string) {
  return page.getByText(`${text} sur le chantier ${site}`, { exact: true });
}

/** The texts of what `a`, `b` and maybe `c` find, in the order of the page. */
function inOrder(a: Locator, b: Locator, c?: Locator) {
  return (c ? a.or(b).or(c) : a.or(b)).allTextContents();
}

/** Opens `site`'s conversation; its note, once shown. */
async function open(page: Page, site: string, note: string) {
  await page.getByRole('link', { name: `Suivi du chantier ${site}` }).click();
  const shown = page.getByRole('note').filter({ hasText: note });
  await expect(shown).toBeVisible();
  return shown;
}

test.beforeEach(async ({ page }) => {
  await signIn(page, demo.marc);
});

test('the messages summarised above are said so, and the summary shown', async ({ page }) => {
  const note = await open(
    page,
    'Lumière',
    'Les 12 messages ci-dessus sont résumés pour l’assistant',
  );
  // After the last message it covers, before the first it does not.
  const around = await inOrder(
    message(page, 'Réponse 6', 'Lumière'),
    note,
    message(page, 'Question 7', 'Lumière'),
  );
  expect(around).toHaveLength(3);
  expect(around[0]).toBe('Réponse 6 sur le chantier Lumière');
  expect(around[1]).toContain('Les 12 messages ci-dessus');
  expect(around[2]).toBe('Question 7 sur le chantier Lumière');

  // Caught up: nothing said of messages left out.
  await expect(note).not.toContainText('pas encore résumé');

  // The summary itself, on demand.
  const summary = 'Marc suit le chantier Lumière : INT-2049 reste en cours';
  await expect(page.getByText(summary)).toBeHidden();
  await note.getByText('Voir le résumé').click();
  await expect(note.getByText(summary)).toBeVisible();
  await noError(page);
});

test('summarised messages no longer shown are said so first, or counted', async ({ page }) => {
  // None shown: the summary stops before the first message shown.
  const first = await open(
    page,
    'Horizon',
    'Les 8 premiers messages de cette conversation sont résumés pour l’assistant',
  );
  await expect(message(page, 'Réponse 5', 'Horizon')).toHaveCount(0);
  const opening = await inOrder(first, message(page, 'Question 6', 'Horizon'));
  expect(opening).toHaveLength(2);
  expect(opening[0]).toContain('Les 8 premiers messages');
  expect(opening[1]).toBe('Question 6 sur le chantier Horizon');
  // Lagging 202 messages behind, of which the last 20 are reread.
  await expect(first).toContainText(
    'Les 182 messages suivants ne sont pas encore résumés, et l’assistant ne les relit pas : ils le seront après ses prochaines réponses.',
  );
  await first.getByText('Voir le résumé').click();
  await expect(first.getByText('réserves levées')).toBeVisible();

  // Some shown: after the last it covers, counting those no longer shown.
  const counted = await open(
    page,
    'Azur',
    'Les 20 premiers messages de cette conversation, jusqu’au message ci-dessus, sont résumés pour l’assistant',
  );
  const between = await inOrder(
    message(page, 'Réponse 10', 'Azur'),
    counted,
    message(page, 'Question 11', 'Azur'),
  );
  expect(between).toHaveLength(3);
  expect(between[0]).toBe('Réponse 10 sur le chantier Azur');
  expect(between[1]).toContain('Les 20 premiers messages');
  expect(between[2]).toBe('Question 11 sur le chantier Azur');
  await expect(counted).toContainText('Les 170 messages suivants ne sont pas encore résumés');
  await noError(page);
});

test('a summary lagging by a single message says so', async ({ page }) => {
  // Ten messages summarised, twenty-one after them, the latest twenty reread.
  const note = await open(
    page,
    'Écluse',
    'Les 10 messages ci-dessus sont résumés pour l’assistant',
  );
  await expect(note).toContainText(
    'Le message suivant n’est pas encore résumé, et l’assistant ne le relit pas : il le sera après sa prochaine réponse.',
  );
  await noError(page);
});
