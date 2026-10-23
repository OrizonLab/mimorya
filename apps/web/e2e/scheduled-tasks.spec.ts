// A person's scheduled tasks (ADR-080, ADR-081): Léa reads what became of
// the occurrence a stop of the service made miss (the server leaves it at
// startup), then schedules a request, pauses and resumes it, moves it and
// deletes it, each change read back from the API. Her test task runs twelve
// hours from now at the earliest: none fires while the suite lasts; it is
// deleted whatever happens. The rules themselves are tested on the API
// (`crates/api/tests/scheduled_task*.rs`).
import type { Page } from '@playwright/test';
import type { ScheduledTask, ScheduledTaskList } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const TITLE = 'Point des retards (e2e)';
const INSTRUCTION = 'Signale les interventions en retard.';
/** The task the server leaves, its occurrence missed. */
const MISSED = 'Relevé du soir';

/** The page the test signed in, for its cleanup. */
const left: { page?: Page } = {};

test.afterEach(async () => {
  const page = left.page;
  left.page = undefined;
  if (!page) return;
  for (const task of await tasks(page)) {
    if (task.title !== TITLE) continue;
    const deleted = await page.request.delete(`/api/scheduled-tasks/${task.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

/** The tasks of whom `page` signed in, as the API records them. */
async function tasks(page: Page): Promise<ScheduledTask[]> {
  const listed = await page.request.get('/api/scheduled-tasks');
  expect(listed.status()).toBe(200);
  return ((await listed.json()) as ScheduledTaskList).tasks;
}

async function task(page: Page, title: string): Promise<ScheduledTask | undefined> {
  return (await tasks(page)).find((t) => t.title === title);
}

/** `HH:MM`, `hours` from now, on the clock of the browser (Node's too). */
function inHours(hours: number): string {
  const then = new Date(Date.now() + hours * 3_600_000);
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${pad(then.getHours())}:${pad(then.getMinutes())}`;
}

/** `HH:MM` as the screen says it: « 8 h 05 ». */
function spoken(time: string): string {
  const [hours, minutes] = time.split(':');
  return `${Number(hours)} h ${minutes}`;
}

test('a person schedules a request, pauses, moves and deletes it', async ({ page }) => {
  await signIn(page, demo.lea);
  left.page = page;
  await page.getByRole('link', { name: 'Tâches planifiées' }).click();
  await expect(page.getByRole('heading', { name: 'Tâches planifiées' })).toBeVisible();

  // The occurrence a stop made miss, as the server's first pass recorded it.
  await expect.poll(async () => (await task(page, MISSED))?.lastOccurrence?.outcome).toBe('missed');
  await page.reload();
  const missed = page.getByRole('article', { name: MISSED });
  await expect(missed.getByText('Non rattrapée')).toBeVisible();
  await expect(missed.getByText(/n’a pas pu être lancée dans l’heure/)).toBeVisible();
  await expect(missed.getByText(/^Tous les jours à /)).toBeVisible();

  // Monday, Wednesday and Friday, twelve hours from now.
  const form = page.getByRole('region', { name: 'Nouvelle tâche' });
  const time = inHours(12);
  await form.getByLabel('Intitulé').fill(TITLE);
  await form.getByLabel('Demande à l’agent').fill(INSTRUCTION);
  await form.getByLabel('mardi', { exact: true }).uncheck();
  await form.getByLabel('jeudi', { exact: true }).uncheck();
  await form.getByLabel('Heure').fill(time);
  await form.getByRole('button', { name: 'Planifier' }).click();
  const card = page.getByRole('article', { name: TITLE });
  await expect(
    card.getByText(`Chaque lundi, mercredi et vendredi à ${spoken(time)}`),
  ).toBeVisible();
  await expect(form.getByLabel('Intitulé')).toHaveValue('');
  const saved = await task(page, TITLE);
  expect(saved).toMatchObject({
    instruction: INSTRUCTION,
    weekdays: [1, 3, 5],
    time,
    enabled: true,
  });
  expect(saved?.nextRunAt).not.toBeNull();

  // Paused, then resumed.
  await card.getByRole('button', { name: `Mettre en pause « ${TITLE} »` }).click();
  await expect(card.getByText('En pause', { exact: true })).toBeVisible();
  expect(await task(page, TITLE)).toMatchObject({ enabled: false, nextRunAt: null });
  await card.getByRole('button', { name: `Reprendre « ${TITLE} »` }).click();
  await expect(card.getByText('Active', { exact: true })).toBeVisible();
  expect((await task(page, TITLE))?.enabled).toBe(true);

  // Moved to an hour later, on Saturdays too.
  const later = inHours(13);
  await card.getByRole('button', { name: `Modifier « ${TITLE} »` }).click();
  await card.getByLabel('Heure').fill(later);
  await card.getByLabel('samedi', { exact: true }).check();
  await card.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(
    card.getByText(`Chaque lundi, mercredi, vendredi et samedi à ${spoken(later)}`),
  ).toBeVisible();
  expect(await task(page, TITLE)).toMatchObject({ weekdays: [1, 3, 5, 6], time: later });

  await card.getByRole('button', { name: `Supprimer « ${TITLE} »` }).click();
  await expect(page.getByRole('article', { name: TITLE })).toHaveCount(0);
  expect(await task(page, TITLE)).toBeUndefined();
  await noError(page);
});
