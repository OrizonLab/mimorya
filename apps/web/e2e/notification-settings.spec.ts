// The e-mails each person chooses to receive (ADR-094): Emma stops those
// telling of her missions' end from « Mon compte », read back once the
// page is reloaded, the others still received. Whatever happens, she
// receives them all again. Which notices follow the choice is tested on
// the API (`crates/api/tests/notification_settings.rs`).
import type { Page } from '@playwright/test';
import type { NotificationSettings } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const ENDS = 'La fin de mes missions';
const APPROVALS = 'Les demandes qui attendent ma décision';
const OWN_RUNS = 'Mes tâches planifiées, missions et déclencheurs qui attendent ma validation';

/** Who the test signed in, for its cleanup. */
const left: { emma?: Page } = {};

test.afterEach(async () => {
  if (left.emma) {
    const all = await left.emma.request.put('/api/account/notifications', {
      headers: WRITE,
      data: { approvals: true, ownRuns: true, missionEnds: true } satisfies NotificationSettings,
    });
    expect(all.status()).toBe(200);
  }
  left.emma = undefined;
});

test('one chooses the e-mails one receives', async ({ page }) => {
  await signIn(page, demo.emma);
  left.emma = page;
  await page.goto('/account');
  const form = page.getByRole('form', { name: 'E-mails reçus' });
  for (const kind of [APPROVALS, OWN_RUNS, ENDS]) {
    await expect(form.getByRole('checkbox', { name: kind })).toBeChecked();
  }
  await expect(form.getByRole('button', { name: 'Enregistrer' })).toBeDisabled();
  await form.getByRole('checkbox', { name: ENDS }).uncheck();
  const saved = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/account/notifications') &&
      response.request().method() === 'PUT',
  );
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  expect((await saved).status()).toBe(200);
  await expect(form.getByRole('status')).toHaveText('Choix enregistré.');
  await expect(form.getByRole('button', { name: 'Enregistrer' })).toBeDisabled();

  // Read back from the API once the page is reloaded.
  await page.reload();
  await expect(form.getByRole('checkbox', { name: ENDS })).not.toBeChecked();
  await expect(form.getByRole('checkbox', { name: APPROVALS })).toBeChecked();
  await expect(form.getByRole('checkbox', { name: OWN_RUNS })).toBeChecked();
  const read = await page.request.get('/api/account/notifications');
  expect((await read.json()) as NotificationSettings).toEqual({
    approvals: true,
    ownRuns: true,
    missionEnds: false,
  });
  await noError(page);
});
