// Lessons drawn from finished conversations (ADR-076): Paul has them
// proposed from « Modèle d’IA »; Marc's request ends, refused, and the
// rereading proposes a lesson in his own memory, which he validates, then
// deletes. Whatever happens, the setting is turned off again, the lesson
// deleted and a validation left waiting refused. The rules themselves are
// tested on the API (`crates/api/tests/lesson_capture.rs`).
import type { Page } from '@playwright/test';
import type { MemoryList, MemorySettingsUpdate, MemorySettingsView } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { refuseWaiting } from './support/approvals';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { REFERENCE_REQUEST } from './support/scenario';

const WRITE = { 'X-Mimorya-Request': '1' };
/** The lesson the server's model draws from any run (`e2e_server/lessons.rs`). */
const LESSON = 'Vérifier le planning avant de proposer un suivi';
const CAPTURE = 'Proposer à chacun les leçons de ses conversations';

/** Who the test signed in, for its cleanup. */
const left: { paul?: Page; marc?: Page } = {};

/** Marc's memories titled as the lesson. */
async function lessons(marc: Page) {
  const listed = await marc.request.get('/api/memory');
  expect(listed.status()).toBe(200);
  const { items } = (await listed.json()) as MemoryList;
  return items.filter((m) => m.title === LESSON);
}

test.afterEach(async () => {
  const { paul, marc } = left;
  // Turned off first: a run refused below is reread by nobody.
  if (paul) {
    const off = await paul.request.put('/api/admin/memory-settings', {
      headers: WRITE,
      data: { captureLessons: false } satisfies MemorySettingsUpdate,
    });
    expect(off.status()).toBe(200);
  }
  if (marc) {
    await refuseWaiting(marc);
    for (const lesson of await lessons(marc)) {
      const deleted = await marc.request.delete(`/api/memory/${lesson.id}`, { headers: WRITE });
      expect(deleted.status()).toBe(200);
    }
  }
  left.paul = left.marc = undefined;
});

test('a finished conversation leaves a lesson its user validates', async ({ newContext }) => {
  const paul = await (await newContext()).newPage();
  await openAdmin(paul, 'Modèle d’IA');
  left.paul = paul;
  const settings = paul.getByRole('form', { name: 'Leçons tirées des conversations' });
  await expect(settings.getByRole('checkbox', { name: CAPTURE })).not.toBeChecked();
  await settings.getByRole('checkbox', { name: CAPTURE }).check();
  await settings.getByRole('button', { name: 'Enregistrer' }).click();
  // Recorded before the page reloads.
  await expect
    .poll(async () => {
      const shown = await paul.request.get('/api/admin/memory-settings');
      return ((await shown.json()) as MemorySettingsView).captureLessons;
    })
    .toBe(true);
  await paul.reload();
  await paul.getByRole('tab', { name: 'Modèle d’IA' }).click();
  await expect(settings.getByRole('checkbox', { name: CAPTURE })).toBeChecked();
  await noError(paul);

  // Marc's request ends, refused: the conversation is reread.
  const marc = await (await newContext()).newPage();
  await signIn(marc, demo.marc);
  left.marc = marc;
  await marc.getByLabel('Votre message').fill(REFERENCE_REQUEST);
  await marc.getByRole('button', { name: 'Envoyer' }).click();
  await expect(marc.getByText('Validation requise')).toBeVisible({ timeout: 30_000 });
  await marc.getByRole('button', { name: 'Refuser' }).click();
  await expect(
    marc.getByText('Le planning est inchangé : la validation a été refusée.'),
  ).toBeVisible({ timeout: 30_000 });
  await expect.poll(async () => (await lessons(marc)).map((m) => m.status)).toEqual(['proposed']);

  // Proposed in his own memory, used once he validates it.
  await marc.goto('/memory');
  const proposal = marc.getByRole('listitem').filter({ hasText: LESSON });
  await expect(proposal.getByText('Leçon', { exact: true })).toBeVisible();
  await expect(proposal.getByText('Ma mémoire', { exact: true })).toBeVisible();
  await marc.getByRole('button', { name: `Valider « ${LESSON} »` }).click();
  await expect(marc.getByText('Propositions à valider')).toHaveCount(0);
  await marc.reload();
  await expect(marc.getByRole('button', { name: `Supprimer « ${LESSON} »` })).toBeVisible();
  expect((await lessons(marc)).map((m) => m.status)).toEqual(['active']);

  await marc.getByRole('button', { name: `Supprimer « ${LESSON} »` }).click();
  await expect(marc.getByText(LESSON, { exact: true })).toHaveCount(0);
  expect(await lessons(marc)).toEqual([]);
  await noError(marc);
});
