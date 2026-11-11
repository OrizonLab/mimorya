// A company's own retention durations (ADR-195, ADR-196): Paul shortens its
// episodes in « Conservation des données ». The page asks first (declined,
// nothing is sent), then says, of each duration, whose it is and, of the
// shorter one, the day the purge applies it and what it keeps meanwhile,
// read back once the page is reloaded. Another duration shortened, only
// that one is asked about, not the one waiting. Back to the platform's, a
// lengthening, it applies at once without asking. A page left open while
// the durations change elsewhere saves nothing, and says so. Whatever
// happens, the platform's durations are set again: their values, not their
// origin, a duration the company chose staying its own (the API removes
// none, ADR-196). The rules themselves are tested on the API
// (`crates/api/tests/retention_durations.rs`); who opens the tab, in
// `retention-access.spec.ts`.
import type { Page } from '@playwright/test';
import type { RetentionUpdate, RetentionView } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const TAB = 'Conservation des données';
/** How many days a shortening waits (`SHORTENING_DELAY_DAYS`). */
const DELAY_DAYS = 7;
const OWN = 'Durée choisie par votre entreprise.';
const PLATFORM_OWN = 'Durée de la plateforme, faute d’un choix de votre entreprise.';
const CHANGED =
  'Les durées de conservation ont changé depuis l’ouverture de cette page : ' +
  'rechargez-la pour voir les durées actuelles';

/** Durations, as a change asks them. */
type Durations = Omit<RetentionUpdate, 'replaces'>;
const PLATFORM: Durations = { conversationMonths: 24, episodeMonths: 12, departedDays: 30 };

/** Who the test signed in, for its cleanup. */
const left: { paul?: Page } = {};

/** `page` sets `durations` through the API, on the version it reads first. */
async function setDurations(page: Page, durations: Durations) {
  const read = (await (await page.request.get('/api/admin/retention')).json()) as RetentionView;
  const set = await page.request.put('/api/admin/retention', {
    headers: WRITE,
    data: {
      conversationMonths: durations.conversationMonths,
      episodeMonths: durations.episodeMonths,
      departedDays: durations.departedDays,
      replaces: read.version,
    } satisfies RetentionUpdate,
  });
  expect(set.status()).toBe(200);
}

test.afterEach(async () => {
  if (left.paul) await setDurations(left.paul, PLATFORM);
  left.paul = undefined;
});

/** The day, as the page writes it, `DELAY_DAYS` after `at`. */
function inAWeek(page: Page, at: number): Promise<string> {
  return page.evaluate(
    (day) => new Date(day).toLocaleDateString('fr-FR', { dateStyle: 'short' }),
    at + DELAY_DAYS * 24 * 60 * 60 * 1000,
  );
}

test('a shortening is asked first, then waits its week', async ({ page }) => {
  await openAdmin(page, TAB);
  left.paul = page;
  // Every duration the page sends, to tell a declined one sent nothing.
  const sent: string[] = [];
  page.on('request', (request) => {
    if (request.method() === 'PUT' && request.url().endsWith('/api/admin/retention')) {
      sent.push(request.postData() ?? '');
    }
  });
  const form = page.getByRole('form', { name: 'Durées de conservation' });
  const save = form.getByRole('button', { name: 'Enregistrer' });
  const conversations = form.getByLabel('Conversations inactives (mois)');
  const episodes = form.getByLabel('Mémoires épisodiques (mois)');
  await expect(conversations).toHaveValue('24');
  await expect(episodes).toHaveValue('12');
  await expect(form.getByLabel('Mémoires personnelles d’un compte désactivé (jours)')).toHaveValue(
    '30',
  );
  await expect(form.getByText(PLATFORM_OWN, { exact: true })).toHaveCount(3);
  // Who reaches a field hears its help: whose it is and its bounds.
  await expect(episodes).toHaveAccessibleDescription(
    `${PLATFORM_OWN} De 1 à 36 mois ; plateforme : 12 mois.`,
  );
  // The duration saved, written otherwise: nothing to save.
  await episodes.fill('012');
  await expect(save).toBeDisabled();

  // Shorter: asked first, and declined, nothing is sent.
  await episodes.fill('6');
  const asked: string[] = [];
  page.once('dialog', (dialog) => {
    asked.push(dialog.message());
    void dialog.dismiss();
  });
  await save.click();
  await expect.poll(() => asked.length).toBe(1);
  expect(asked[0]).toContain('Mémoires épisodiques : 6 mois');
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(episodes).toHaveValue('12');
  expect(sent).toEqual([]);

  // Accepted: the purge keeps twelve months until the day the shorter one
  // applies, a week after it was saved.
  await episodes.fill('6');
  page.once('dialog', (dialog) => void dialog.accept());
  const before = Date.now();
  await save.click();
  const waiting = form.getByText(/^Raccourcie à 6 mois : la purge l’applique à partir du/);
  await expect(waiting).toBeVisible();
  const days = [await inAWeek(page, before), await inAWeek(page, Date.now())];
  await expect(waiting).toHaveText(
    new RegExp(`à partir du (${days.join('|')}),?\\s\\d{2}:\\d{2}, et garde d’ici là 12 mois\\.$`),
  );
  // Episodes alone are the company's; the others still the platform's.
  await expect(form.getByText(OWN, { exact: true })).toHaveCount(1);
  await expect(form.getByText(PLATFORM_OWN, { exact: true })).toHaveCount(2);
  // The field's help says the waiting shortening too.
  await expect(episodes).toHaveAccessibleDescription(
    new RegExp(
      `^${OWN.replace('.', '\\.')} De 1 à 36 mois ; plateforme : 12 mois\\. Raccourcie à 6 mois : `,
    ),
  );

  // Read back from the API once the page is reloaded.
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(episodes).toHaveValue('6');
  await expect(waiting).toBeVisible();
  await expect(form.getByText(OWN, { exact: true })).toHaveCount(1);
  await noError(page);

  // Conversations shortened, the waiting episodes left as they are: only
  // the conversations are asked about. Declined, nothing is sent.
  await conversations.fill('12');
  page.once('dialog', (dialog) => {
    asked.push(dialog.message());
    void dialog.dismiss();
  });
  await save.click();
  await expect.poll(() => asked.length).toBe(2);
  expect(asked[1]).toContain('Conversations inactives : 12 mois');
  expect(asked[1]).not.toContain('Mémoires épisodiques');
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(conversations).toHaveValue('24');
  expect(sent).toHaveLength(1);

  // Back to the platform's, a lengthening: applied at once, nothing asked.
  page.on('dialog', (dialog) => {
    throw new Error(`asked for a lengthening: ${dialog.message()}`);
  });
  await episodes.fill('12');
  await save.click();
  await expect(waiting).toHaveCount(0);
  await expect(save).toBeDisabled();
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(episodes).toHaveValue('12');
  await expect(waiting).toHaveCount(0);
  expect(sent).toHaveLength(2);
  await noError(page);
});

test('a page left open saves nothing once the durations changed', async ({ page }) => {
  await openAdmin(page, TAB);
  left.paul = page;
  const form = page.getByRole('form', { name: 'Durées de conservation' });
  const episodes = form.getByLabel('Mémoires épisodiques (mois)');
  const departed = form.getByLabel('Mémoires personnelles d’un compte désactivé (jours)');
  await expect(departed).toHaveValue('30');

  // Meanwhile, the departed accounts' memories are shortened elsewhere.
  await setDurations(page, { ...PLATFORM, departedDays: 20 });

  // On what it read, the page shortens the episodes: it would send the
  // departed accounts' 30 days back. Refused and said; nothing changes.
  await episodes.fill('6');
  page.once('dialog', (dialog) => void dialog.accept());
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(form.getByText(CHANGED)).toBeVisible();
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(episodes).toHaveValue('12');
  await expect(departed).toHaveValue('20');
  await expect(form.getByText(/^Raccourcie à 20 jours : la purge l’applique/)).toBeVisible();
  await noError(page);
});
