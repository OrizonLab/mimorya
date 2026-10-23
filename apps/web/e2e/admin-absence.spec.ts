// An account administrator declares the absence of a collaborator who
// cannot (ADR-078, ADR-079): Paul declares Marc's from « Utilisateurs »,
// Julien standing in; Marc, told by e-mail, reads in « Validations » who
// declared it and ends it himself. Paul declares it again, then lifts it.
// Whatever happens, Marc is left without an absence. The rules themselves
// are tested on the API (`crates/api/tests/admin_absence*.rs`).
import type { Page } from '@playwright/test';
import type { CollaboratorAbsence } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { lastLink, mailCount } from './support/mail';

const WRITE = { 'X-Mimorya-Request': '1' };
const MARC = '00000000-0000-4000-8000-000000000201';
const JULIEN = '00000000-0000-4000-8000-000000000202';
const ABSENCE = `/api/admin/users/${MARC}/absence`;

/** The administrator the test signed in, for its cleanup. */
const left: { paul?: Page } = {};

test.afterEach(async () => {
  // 404: no absence left.
  if (left.paul) {
    const lifted = await left.paul.request.delete(ABSENCE, { headers: WRITE });
    expect([200, 404]).toContain(lifted.status());
  }
  left.paul = undefined;
});

/** The day `days` from today, as a date field takes it. */
function inDays(days: number): string {
  const day = new Date(Date.now() + days * 86_400_000);
  return new Date(day.getTime() - day.getTimezoneOffset() * 60_000).toISOString().slice(0, 10);
}

/** Marc's absence, as the API records it. */
async function absence(paul: Page) {
  const shown = await paul.request.get(ABSENCE);
  expect(shown.status()).toBe(200);
  return ((await shown.json()) as CollaboratorAbsence).absence;
}

/** Opens Marc's account in « Utilisateurs », the page loaded afresh. */
async function openMarc(paul: Page) {
  await paul.reload();
  await paul.getByRole('tab', { name: 'Utilisateurs' }).click();
  await expect(paul.getByText('Chargement…')).toHaveCount(0);
  const row = paul.getByRole('row').filter({ hasText: 'Marc Lefèvre' }).first();
  await row.getByRole('button', { name: 'Gérer' }).click();
}

/** Declares Marc's absence for three days, Julien standing in. */
async function declare(paul: Page) {
  await paul.getByLabel('Remplaçant').selectOption({ label: 'Julien Moreau' });
  await paul.getByLabel('Au (inclus)').fill(inDays(3));
  await paul.getByRole('button', { name: 'Déclarer son absence' }).click();
  await expect(paul.getByText('Un e-mail en informe Marc Lefèvre.')).toBeVisible();
}

test('an administrator declares the absence of a collaborator who cannot', async ({
  page,
  newContext,
}) => {
  const paul = await (await newContext()).newPage();
  await openAdmin(paul, 'Utilisateurs');
  left.paul = paul;
  await openMarc(paul);
  const before = await mailCount(demo.marc);
  await declare(paul);
  const declared = await absence(paul);
  expect(declared?.delegate.id).toBe(JULIEN);
  expect(declared?.declaredBy?.displayName).toBe('Paul Renaud');
  // Until the end of the last day, in the browser's time zone (Node's too).
  expect(declared?.endsAt).toBe(new Date(`${inDays(3)}T23:59:59`).toISOString());
  await openMarc(paul);
  await expect(
    paul.getByText(
      /^Julien Moreau remplace Marc Lefèvre du .* Absence déclarée par Paul Renaud\.$/,
    ),
  ).toBeVisible();

  // Marc, told by e-mail, reads who declared it and ends it himself.
  const { link, mail } = await lastLink(demo.marc, 'approvals', before);
  expect(mail.subject).toBe('Paul Renaud a déclaré votre absence');
  await signIn(page, demo.marc);
  await page.goto(link);
  await expect(
    page.getByText('Absence déclarée pour vous par Paul Renaud, depuis l’administration.'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Mettre fin à l’absence' }).click();
  await expect(page.getByRole('button', { name: 'Déclarer mon absence' })).toBeVisible();
  expect(await absence(paul)).toBeNull();

  // Declared again, lifted by Paul once he confirms it.
  await openMarc(paul);
  await declare(paul);
  paul.on('dialog', (dialog) => dialog.accept());
  await paul.getByRole('button', { name: 'Lever l’absence' }).click();
  await expect(paul.getByRole('button', { name: 'Déclarer son absence' })).toBeVisible();
  await expect(paul.getByText('Un e-mail en informe Marc Lefèvre.')).toBeVisible();
  expect(await absence(paul)).toBeNull();
  const lifted = await lastLink(demo.marc, 'approvals', before + 2);
  expect(lifted.mail.subject).toBe('Paul Renaud a mis fin à votre absence');
  await noError(paul);
  await noError(page);
});
