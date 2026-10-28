// The company's Microsoft 365 application and the accounts its people link
// there (ADR-116, ADR-118), its tools reading only: Paul declares the
// application (its secret never listed), is shown the return address to
// register in it, checks the declaration (Microsoft is not reached) and
// sets a tool's rule; Léa, from « Mon compte », links her account at the
// test provider, which plays Microsoft and comes back to her page, and is
// told what the agent may do there; Paul sees her link counted and told in
// the deletion he refuses, renames the application (its writing option not
// sent, unchanged) and disables it; Léa is told her link no longer serves
// and removes it; Paul deletes the application. Each step is read back from
// the API, each confirmation read. The test server sends people to its
// provider instead of Microsoft; the tools are never called. Whatever
// happens, the application is deleted, with its links. Writing:
// `microsoft365-writing.spec.ts`.
import { demo, expect, noError, signIn, test } from './support/fixtures';
import {
  APPLICATION,
  DIRECTORY,
  SECRET,
  declare,
  deleteApplications,
  linkOf,
  linkThrough,
  stored,
} from './support/microsoft365';
import { cardWith } from './support/admin';

const NAME = 'Microsoft 365 e2e';
const RENAMED = 'Messagerie e2e';
const READ = 'm365_mail_read';

test.afterEach(async ({ page }) => deleteApplications(page));

test('an application is declared, an account linked and removed, the application deleted', async ({
  page,
  newContext,
}) => {
  // Each confirmation is read; Paul's first deletion is refused.
  const asked: string[] = [];
  let refuse = true;
  page.on('dialog', (dialog) => {
    asked.push(dialog.message());
    void (refuse ? dialog.dismiss() : dialog.accept());
  });
  const card = await declare(page, NAME, false);
  await expect(card.getByText('0 compte(s) relié(s)')).toBeVisible();
  await expect(card.getByText(/^Lecture seule/)).toBeVisible();
  // One per company: no second declaration is offered.
  await expect(
    page.getByRole('button', { name: 'Déclarer l’application Microsoft 365' }),
  ).toHaveCount(0);
  await noError(page);
  const { app, listing } = await stored(page);
  expect(app?.name).toBe(NAME);
  expect(app?.microsoft365?.directory).toBe(DIRECTORY);
  expect(app?.microsoft365?.clientId).toBe(APPLICATION);
  expect(app?.microsoft365?.secretStored).toBe(true);
  expect(app?.microsoft365?.writeTools).toBe(false);
  expect(listing).not.toContain(SECRET);

  // Its declaration checked without reaching Microsoft, a tool's rule set.
  await expect(card.getByText('Non vérifiée', { exact: true })).toBeVisible();
  await card.getByRole('button', { name: 'Vérifier la déclaration' }).click();
  await expect(
    card.getByText(/^Déclaration vérifiée : 3 outil\(s\)\. Microsoft n’est pas appelé/),
  ).toBeVisible();
  await card.getByLabel(`Risque de ${READ}`).selectOption('write');
  await card.getByRole('button', { name: 'Enregistrer les règles des outils' }).click();
  await expect.poll(async () => (await stored(page)).app?.toolPolicies[READ]?.risk).toBe('write');
  await expect(card.getByText('Déclaration vérifiée', { exact: true })).toBeVisible();

  // Léa links her own account, at the provider and back to her page.
  const lea = await (await newContext()).newPage();
  const leaAsked: string[] = [];
  lea.on('dialog', (dialog) => {
    leaAsked.push(dialog.message());
    void dialog.accept();
  });
  await signIn(lea, demo.lea);
  await lea.goto('/account');
  const section = lea.getByRole('region', { name: NAME });
  await expect(section.getByText('Non relié')).toBeVisible();
  await linkThrough(lea, section, 'Relier mon compte');
  await expect(lea.getByText('Votre compte est relié')).toBeVisible();
  await expect(section.getByText(`Relié à ${demo.lea}`)).toBeVisible();
  await expect(
    section.getByText(/L’agent peut : lire vos e-mails, lire votre agenda\./),
  ).toBeVisible();
  await expect(section.getByRole('button', { name: 'Relier de nouveau' })).toBeVisible();
  await noError(lea);
  expect((await linkOf(lea))?.linked?.address).toBe(demo.lea);
  expect((await linkOf(lea))?.linked?.can).toEqual(['Mail.Read', 'Calendars.Read']);
  // Said once: the page reloaded no longer says it.
  await lea.reload();
  await expect(section.getByText(`Relié à ${demo.lea}`)).toBeVisible();
  await expect(lea.getByText('Votre compte est relié')).toHaveCount(0);

  // Paul sees it counted; a deletion says it goes with the application, and
  // refused, deletes nothing.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await expect(card.getByText('1 compte(s) relié(s)')).toBeVisible();
  await card.getByRole('button', { name: 'Supprimer' }).click();
  await expect.poll(() => asked.length).toBe(1);
  expect(asked[0]).toContain('1 compte(s)');
  expect((await stored(page)).app?.name).toBe(NAME);
  expect((await linkOf(lea))?.linked?.address).toBe(demo.lea);
  refuse = false;

  // Renamed: its writing option, unchanged, is not sent (the audit says
  // only what changes).
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const edit = page.getByRole('form', { name: 'Application Microsoft 365' });
  await edit.getByLabel('Nom', { exact: true }).fill(RENAMED);
  const sent = page.waitForRequest((r) => r.method() === 'PATCH');
  await edit.getByRole('button', { name: 'Enregistrer' }).click();
  expect((await sent).postDataJSON()).toEqual({ name: RENAMED });
  const renamedCard = cardWith(page, RENAMED, 'Supprimer');
  await expect(renamedCard.getByText(/^Lecture seule/)).toBeVisible();
  expect((await stored(page)).app?.name).toBe(RENAMED);
  await lea.reload();
  const renamed = lea.getByRole('region', { name: RENAMED });
  await expect(renamed.getByText(`Relié à ${demo.lea}`)).toBeVisible();

  // Disabled: Léa's link no longer serves, she may remove it.
  await renamedCard.getByRole('button', { name: 'Désactiver' }).click();
  await expect.poll(async () => (await stored(page)).app?.enabled).toBe(false);
  await lea.reload();
  await expect(renamed.getByText(/L’application est désactivée/)).toBeVisible();
  await expect(renamed.getByRole('button', { name: /^Relier/ })).toHaveCount(0);
  // Not said to do anything there any more.
  await expect(renamed.getByText(/L’agent peut/)).toHaveCount(0);
  expect((await linkOf(lea))?.linked?.can).toEqual([]);
  await renamed.getByRole('button', { name: 'Retirer la liaison' }).click();
  await expect(lea.getByRole('region', { name: RENAMED })).toHaveCount(0);
  expect(leaAsked).toHaveLength(1);
  expect(leaAsked[0]).toContain('reste chez Microsoft');
  expect((await linkOf(lea))?.linked ?? null).toBeNull();

  // Paul deletes the application.
  await renamedCard.getByRole('button', { name: 'Supprimer' }).click();
  await expect(page.getByText(RENAMED)).toHaveCount(0);
  expect((await stored(page)).app).toBeUndefined();
  expect(asked).toHaveLength(2);
});

test('a failed return is said on « Mon compte », once', async ({ page }) => {
  await signIn(page, demo.lea);
  await page.goto('/account?liaison=echec');
  await expect(page.getByText('La liaison a échoué')).toBeVisible();
  await expect(page).toHaveURL(/\/account$/);
  await page.reload();
  await expect(page.getByText('La liaison a échoué')).toHaveCount(0);
});
