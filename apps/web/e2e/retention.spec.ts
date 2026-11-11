// A company's own retention durations (ADR-195, ADR-196): Paul shortens its
// episodes in « Conservation des données ». The page asks first (declined,
// nothing is saved), then says when the purge applies the shorter one and
// what it keeps meanwhile, read back once the page is reloaded. Back to the
// platform's, a lengthening, it applies at once without asking. Whatever
// happens, the platform's durations are set again. The rules themselves are
// tested on the API (`crates/api/tests/retention_durations.rs`).
import type { Page } from '@playwright/test';
import type { RetentionUpdate } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const TAB = 'Conservation des données';
const PLATFORM = {
  conversationMonths: 24,
  episodeMonths: 12,
  departedDays: 30,
} satisfies RetentionUpdate;

/** Who the test signed in, for its cleanup. */
const left: { paul?: Page } = {};

test.afterEach(async () => {
  if (left.paul) {
    const reset = await left.paul.request.put('/api/admin/retention', {
      headers: WRITE,
      data: PLATFORM,
    });
    expect(reset.status()).toBe(200);
  }
  left.paul = undefined;
});

test('a shortening is asked first, then waits its week', async ({ page }) => {
  await openAdmin(page, TAB);
  left.paul = page;
  const form = page.getByRole('form', { name: 'Durées de conservation' });
  const save = form.getByRole('button', { name: 'Enregistrer' });
  const episodes = form.getByLabel('Mémoires épisodiques (mois)');
  await expect(form.getByLabel('Conversations inactives (mois)')).toHaveValue('24');
  await expect(episodes).toHaveValue('12');
  await expect(form.getByLabel('Mémoires personnelles d’un compte désactivé (jours)')).toHaveValue(
    '30',
  );
  await expect(form.getByText(/^Durées de la plateforme, faute de/)).toBeVisible();
  // The duration saved, written otherwise: nothing to save.
  await episodes.fill('012');
  await expect(save).toBeDisabled();

  // Shorter: asked first, and declined, nothing is saved.
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

  // Accepted: the purge keeps twelve months until the shorter one applies.
  await episodes.fill('6');
  page.once('dialog', (dialog) => void dialog.accept());
  await save.click();
  const waiting = form.getByText(/^Raccourcie à 6 mois : la purge l’applique à partir du/);
  await expect(waiting).toBeVisible();
  await expect(waiting).toContainText('et garde d’ici là 12 mois.');
  await expect(form.getByText(/^Durées propres à votre entreprise\./)).toBeVisible();

  // Read back from the API once the page is reloaded.
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(episodes).toHaveValue('6');
  await expect(waiting).toBeVisible();
  await noError(page);

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
  await noError(page);
});
