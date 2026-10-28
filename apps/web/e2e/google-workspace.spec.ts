// The company's Google Workspace client and the accounts its people link
// there (ADR-121, ADR-124), its tools writing (ADR-123): Paul declares the
// client (its secret never listed), is shown the redirect URI to register
// in it and checks the declaration (Google is not reached); Léa, from « Mon
// compte », is told Google asks her consent, links her Workspace account at
// the test provider, which plays Google and comes back to her page, and is
// told in words what the agent may do there, sending included. Paul takes
// writing back (only that option sent): Léa is told the agent reads only.
// She removes her link, told her consent stays at Google; Paul deletes the
// client. Each step is read back from the API, each confirmation read. The
// test server sends people to its provider instead of Google; the tools
// are never called. Whatever happens, the client is deleted, with its
// links.
import { demo, expect, noError, signIn, test } from './support/fixtures';
import {
  CLIENT,
  SECRET,
  WRITING,
  declare,
  deleteClients,
  linkOf,
  stored,
} from './support/google-workspace';
import { linkThrough } from './support/microsoft365';

const NAME = 'Google Workspace e2e';
const SCOPES = 'https://www.googleapis.com/auth/';
const READING = [`${SCOPES}gmail.readonly`, `${SCOPES}calendar.readonly`];
const WRITING_SCOPES = [`${SCOPES}gmail.send`, `${SCOPES}calendar.events`];

test.afterEach(async ({ page }) => deleteClients(page));

test('a client is declared, an account linked with writing then reading only, the client deleted', async ({
  page,
  newContext,
}) => {
  const asked: string[] = [];
  page.on('dialog', (dialog) => {
    asked.push(dialog.message());
    void dialog.accept();
  });
  const card = await declare(page, NAME, true);
  await expect(card.getByText('0 compte(s) relié(s)')).toBeVisible();
  await expect(card.getByText(/^Envoi d’e-mails et création de rendez-vous permis/)).toBeVisible();
  // One per company: no second declaration is offered.
  await expect(
    page.getByRole('button', { name: 'Déclarer le client Google Workspace' }),
  ).toHaveCount(0);
  await noError(page);
  const { client, listing } = await stored(page);
  expect(client?.name).toBe(NAME);
  expect(client?.googleWorkspace?.clientId).toBe(CLIENT);
  expect(client?.googleWorkspace?.secretStored).toBe(true);
  expect(client?.googleWorkspace?.writeTools).toBe(true);
  expect(listing).not.toContain(SECRET);

  // Its declaration checked without reaching Google: five tools, writing.
  await card.getByRole('button', { name: 'Vérifier la déclaration' }).click();
  await expect(
    card.getByText(/^Déclaration vérifiée : 5 outil\(s\)\. Google n’est pas appelé/),
  ).toBeVisible();

  // Léa links her own Workspace account, at the provider and back.
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
  await expect(
    section.getByText(
      /envoie des e-mails ou crée des rendez-vous en votre nom.*Google vous demande votre accord/,
    ),
  ).toBeVisible();
  await linkThrough(lea, section, 'Relier mon compte');
  await expect(lea.getByText('Votre compte est relié')).toBeVisible();
  await expect(section.getByText(`Relié à ${demo.lea}`)).toBeVisible();
  await expect(
    section.getByText(
      /L’agent peut : lire vos e-mails, lire votre agenda, envoyer des e-mails en votre nom, écrire dans votre agenda\./,
    ),
  ).toBeVisible();
  await noError(lea);
  const linked = (await linkOf(lea))?.linked;
  expect(linked?.address).toBe(demo.lea);
  expect(linked?.can).toEqual([...READING, ...WRITING_SCOPES]);
  expect(linked?.toGrant).toEqual([]);

  // Paul sees it counted, then takes writing back: only that option sent.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await expect(card.getByText('1 compte(s) relié(s)')).toBeVisible();
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const edit = page.getByRole('form', { name: 'Client Google Workspace' });
  await edit.getByRole('checkbox', { name: WRITING }).setChecked(false);
  const sent = page.waitForRequest((r) => r.method() === 'PATCH');
  await edit.getByRole('button', { name: 'Enregistrer' }).click();
  expect((await sent).postDataJSON()).toEqual({ name: NAME, writeTools: false });
  await expect(card.getByText(/^Lecture seule/)).toBeVisible();
  expect((await stored(page)).client?.googleWorkspace?.writeTools).toBe(false);

  // Léa is told the agent reads only; granted, sending is no longer used.
  await lea.reload();
  await expect(
    section.getByText(/L’agent peut : lire vos e-mails, lire votre agenda\./),
  ).toBeVisible();
  expect((await linkOf(lea))?.linked?.can).toEqual(READING);

  // She removes her link, told her consent stays at Google.
  await section.getByRole('button', { name: 'Retirer la liaison' }).click();
  await expect(section.getByText('Non relié')).toBeVisible();
  expect(leaAsked).toHaveLength(1);
  expect(leaAsked[0]).toContain('reste chez Google');
  expect((await linkOf(lea))?.linked ?? null).toBeNull();

  // Paul deletes the client, told what goes with it.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await card.getByRole('button', { name: 'Supprimer' }).click();
  await expect(page.getByText(NAME)).toHaveCount(0);
  expect(asked).toHaveLength(1);
  expect(asked[0]).toContain(`le client « ${NAME} »`);
  expect((await stored(page)).client).toBeUndefined();
});
