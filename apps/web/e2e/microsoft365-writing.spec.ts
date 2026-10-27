// Writing in Microsoft 365, let by the company and granted by each person
// (ADR-119, ADR-120): Paul declares the application with its tools writing;
// Léa, not linked yet, is told the agent may send in her name, each time
// validated. Paul takes it back (the card says read only) and Léa links,
// reading only. Paul lets it again: Léa is asked to grant sending and
// writing her calendar; nothing is asked while the application is
// disabled; enabled again, she links again and the request is gone. Paul
// takes writing back once granted: the page no longer says the agent may
// send. Each step is read back from the API. The application is deleted,
// with her link, whatever happens.
import type { Locator, Page } from '@playwright/test';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import {
  WRITING,
  declare,
  deleteApplications,
  linkOf,
  linkThrough,
  stored,
} from './support/microsoft365';

const NAME = 'Messagerie écrite e2e';
const READING = ['Mail.Read', 'Calendars.Read'];

test.afterEach(async ({ page }) => deleteApplications(page));

/** Paul lets the application's tools write, or takes it back. */
async function setWriting(page: Page, card: Locator, writing: boolean) {
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const form = page.getByRole('form', { name: 'Application Microsoft 365' });
  await form.getByRole('checkbox', { name: WRITING }).setChecked(writing);
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect.poll(async () => (await stored(page)).app?.microsoft365?.writeTools).toBe(writing);
}

/** Paul switches the application on or off. */
async function setEnabled(page: Page, card: Locator, enabled: boolean) {
  await card.getByRole('button', { name: enabled ? 'Activer' : 'Désactiver' }).click();
  await expect.poll(async () => (await stored(page)).app?.enabled).toBe(enabled);
}

test('writing is let, granted by linking again, and taken back', async ({ page, newContext }) => {
  const card = await declare(page, NAME, true);
  await expect(card.getByText(/^Envoi d’e-mails et création de rendez-vous permis/)).toBeVisible();
  expect((await stored(page)).app?.microsoft365?.writeTools).toBe(true);

  // Before she links, Léa is told the agent may send in her name, validated.
  const lea = await (await newContext()).newPage();
  await signIn(lea, demo.lea);
  await lea.goto('/account');
  const section = lea.getByRole('region', { name: NAME });
  const mayWrite = section.getByText(
    /envoie des e-mails ou crée des rendez-vous en votre nom, chaque fois après validation/,
  );
  await expect(mayWrite).toBeVisible();

  // Taken back: read only, on the card and before she links.
  await setWriting(page, card, false);
  await expect(card.getByText(/^Lecture seule/)).toBeVisible();
  await lea.reload();
  await expect(section.getByText(/Reliez votre compte/)).toBeVisible();
  await expect(mayWrite).toHaveCount(0);
  await linkThrough(lea, section, 'Relier mon compte');
  const reading = section.getByText(/L’agent peut : lire vos e-mails, lire votre agenda\.$/);
  await expect(reading).toBeVisible();

  // Let again: she is asked to grant it.
  await setWriting(page, card, true);
  await lea.reload();
  const asking = section.getByText(
    /demande aussi votre accord pour : envoyer des e-mails en votre nom, écrire dans votre agenda\./,
  );
  await expect(asking).toBeVisible();
  await expect(reading).toBeVisible();
  expect((await linkOf(lea))?.linked?.toGrant).toEqual(['Mail.Send', 'Calendars.ReadWrite']);

  // Disabled, nothing is asked of her, nor said possible.
  await setEnabled(page, card, false);
  await lea.reload();
  await expect(section.getByText(/L’application est désactivée/)).toBeVisible();
  await expect(asking).toHaveCount(0);
  await expect(section.getByText(/L’agent peut/)).toHaveCount(0);
  await expect(section.getByRole('button', { name: /^Relier/ })).toHaveCount(0);
  expect((await linkOf(lea))?.linked?.can).toEqual([]);

  // Enabled again, she grants it by linking again.
  await setEnabled(page, card, true);
  await lea.reload();
  await expect(asking).toBeVisible();
  await linkThrough(lea, section, 'Relier de nouveau');
  await expect(
    section.getByText(
      /L’agent peut : lire vos e-mails, lire votre agenda, envoyer des e-mails en votre nom, écrire dans votre agenda\./,
    ),
  ).toBeVisible();
  await expect(asking).toHaveCount(0);
  const granted = (await linkOf(lea))?.linked;
  expect(granted?.toGrant).toEqual([]);
  expect(granted?.can).toEqual([...READING, 'Mail.Send', 'Calendars.ReadWrite']);

  // Taken back once granted: the page no longer says the agent may send.
  await setWriting(page, card, false);
  await lea.reload();
  await expect(reading).toBeVisible();
  expect((await linkOf(lea))?.linked?.can).toEqual(READING);
  await noError(lea);
  await noError(page);
});
