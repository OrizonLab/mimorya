// The company's Teams bot and the accounts people link there (ADR-138,
// ADR-139): Paul declares the bot from « Connecteurs », a secret Entra
// refuses keeping nothing, then its own (never listed), and is shown the
// messaging endpoint to give the Azure bot; Léa is told in « Mon compte »
// how to link her Teams account; she writes to the bot, as the test Bot
// Framework sends it, with its token, and the bot answers with a link.
// Opened in her session, the link shows what it links and links it; she
// is then told the agent is not talked with in Teams yet. Paul sees her
// account counted and disables the bot; the link, reopened, says why it
// links nothing; Léa is told so (and no longer that the bot sends her to
// the application) and removes her link; Paul renames the bot (its
// application and directory never offered; a new secret Entra refuses
// changing nothing, a blank one keeping it) and enables it, after which
// it answers Léa again with the secret kept; he deletes it, and the
// endpoint answers as to an unknown gateway. Each step is read back from
// the API, each confirmation read. Whatever
// happens, the bot is deleted with its links. The rules themselves are
// tested on the API (`crates/api/tests/teams_gateway_*.rs`).
import type { Page } from '@playwright/test';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { cardWith, openAdmin } from './support/admin';
import { deleteGateways } from './support/slack';
import type { AccountGateways } from '../lib/api/contract';
import {
  LEA_TEAMS,
  TEAMS_APP,
  TEAMS_DIRECTORY,
  TEAMS_SECRET,
  botAnswer,
  offeredLink,
  sendActivity,
  storedTeamsBot,
} from './support/teams';

const NAME = 'Teams e2e';
const RENAMED = 'Bot Teams de l’atelier e2e';
const REFUSED_SECRET = 'Abc8Q~fictif.mauvais_secret-0000000000';
const LINKED = 'Votre compte Teams est relié à votre compte.';

test.afterEach(async ({ page }) => deleteGateways(page));

test('a bot is declared, a Teams account linked from its link and removed, the bot deleted', async ({
  page,
  request,
  newContext,
}) => {
  const asked: string[] = [];
  page.on('dialog', (dialog) => {
    asked.push(dialog.message());
    void dialog.accept();
  });
  await openAdmin(page, 'Connecteurs');
  const section = page.getByRole('region', { name: 'Passerelles de conversation' });
  await section.getByRole('button', { name: 'Déclarer le bot Teams' }).click();
  const form = section.getByRole('form', { name: 'Bot Teams' });
  await form.getByLabel('Nom', { exact: true }).fill(NAME);
  await form.getByLabel('ID d’application (Microsoft App ID)').fill(TEAMS_APP);
  await form.getByLabel('ID de l’annuaire (locataire Microsoft Entra)').fill(TEAMS_DIRECTORY);
  // A secret Entra refuses: nothing is kept.
  await form.getByLabel('Secret client').fill(REFUSED_SECRET);
  await form.getByRole('button', { name: 'Déclarer le bot' }).click();
  await expect(form.getByRole('alert')).toContainText('Microsoft Entra refuse l’application');
  expect((await storedTeamsBot(page)).gateway).toBeUndefined();
  await form.getByLabel('Secret client').fill(TEAMS_SECRET);
  await form.getByRole('button', { name: 'Déclarer le bot' }).click();
  const card = cardWith(page, NAME, 'Supprimer');
  await expect(card.getByText('Active', { exact: true })).toBeVisible();
  await expect(
    card.getByText(`Application ${TEAMS_APP} de l’annuaire ${TEAMS_DIRECTORY}`, { exact: false }),
  ).toBeVisible();
  await expect(card.getByText('0 compte(s) relié(s)', { exact: false })).toBeVisible();
  // One per company: no second declaration is offered.
  await expect(section.getByRole('button', { name: 'Déclarer le bot Teams' })).toHaveCount(0);
  await noError(page);
  const { gateway, listing } = await storedTeamsBot(page);
  expect(gateway).toMatchObject({
    name: NAME,
    enabled: true,
    workspaceId: TEAMS_DIRECTORY,
    botUserId: TEAMS_APP,
    secretsStored: true,
    identities: 0,
  });
  expect(listing).not.toContain(TEAMS_SECRET);
  const endpoint = gateway?.eventsUrl ?? '';
  expect(endpoint).toBe(`http://localhost:3000/api/gateways/teams/${gateway?.id}`);
  await expect(card.getByText(endpoint, { exact: true })).toBeVisible();

  // Léa is told how to link her Teams account.
  const lea = await (await newContext()).newPage();
  const leaAsked: string[] = [];
  lea.on('dialog', (dialog) => {
    leaAsked.push(dialog.message());
    void dialog.accept();
  });
  await signIn(lea, demo.lea);
  await lea.goto('/account');
  const row = lea.getByRole('region', { name: NAME });
  await expect(row.getByText('Non relié', { exact: true })).toBeVisible();
  await expect(
    row.getByText('écrivez au bot de l’entreprise dans Teams', { exact: false }),
  ).toBeVisible();

  // She writes to the bot: it answers with a link to her page.
  const offer = await botAnswer(request, endpoint, LEA_TEAMS, 'Bonjour');
  expect(offer).toContain('reliez votre compte Teams à votre compte');
  const link = offeredLink(offer);
  expect(link).toMatch(/^http:\/\/localhost:3000\/account\/gateway-link#.+/);

  // Opened in her session, it shows what it links, then links it.
  await lea.goto(link);
  await expect(lea.getByText(LEA_TEAMS.aad, { exact: true })).toBeVisible();
  await expect(
    lea.getByText(`de l’annuaire « ${TEAMS_DIRECTORY} » (${NAME})`, { exact: false }),
  ).toBeVisible();
  await lea.getByRole('button', { name: 'Relier mon compte Teams' }).click();
  await expect(lea.getByRole('status')).toHaveText(LINKED);
  await noError(lea);
  expect((await identityOn(lea, gateway?.id))?.linked).toMatchObject({
    externalId: LEA_TEAMS.aad,
    address: demo.lea,
  });
  await lea.getByRole('link', { name: 'Mon compte' }).click();
  await expect(row.getByText('Relié', { exact: true })).toBeVisible();
  await expect(
    row.getByText(`Votre compte Teams (${demo.lea}) est relié depuis le`, { exact: false }),
  ).toBeVisible();
  // The bot available, she is told the agent is not answering there yet.
  await expect(row.getByText(/ne sont pas encore ouverts/)).toBeVisible();

  // Linked, she is told the agent is not talked with in Teams yet.
  const told = await botAnswer(request, endpoint, LEA_TEAMS, 'Bonjour');
  expect(told).toContain('Votre compte Teams est relié à votre compte.');
  expect(told).toContain('ne sont pas encore ouverts');

  // Paul sees her account counted, and disables the bot.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await expect(card.getByText('1 compte(s) relié(s)', { exact: false })).toBeVisible();
  await card.getByRole('button', { name: 'Désactiver' }).click();
  await expect(card.getByText('Désactivée', { exact: true })).toBeVisible();
  expect((await storedTeamsBot(page)).gateway?.enabled).toBe(false);

  // The link, reopened, says why it links nothing.
  await lea.goto(link);
  await expect(
    lea.getByText(/^La passerelle Teams de votre entreprise est désactivée/),
  ).toBeVisible();
  await expect(lea.getByRole('button', { name: 'Relier mon compte Teams' })).toHaveCount(0);

  // Léa is told so in « Mon compte », and removes her link.
  await lea.goto('/account');
  await expect(row.getByText(/^La passerelle est désactivée/)).toBeVisible();
  // A disabled bot answers nobody: it is not said to send her anywhere.
  await expect(row.getByText(/ne sont pas encore ouverts/)).toHaveCount(0);
  await row.getByRole('button', { name: 'Retirer la liaison' }).click();
  await expect(row.getByText('Non relié', { exact: true })).toBeVisible();
  await expect(row.getByText('La passerelle est indisponible pour l’instant.')).toBeVisible();
  expect(leaAsked).toEqual([
    `Retirer la liaison de votre compte Teams au bot « ${NAME} » ? Vous pourrez le relier de nouveau depuis le lien que le bot vous enverra.`,
  ]);
  expect((await identityOn(lea, gateway?.id))?.linked).toBeNull();
  await noError(lea);

  // Paul renames it, its application and directory not offered: with a new
  // secret Entra refuses, nothing changes; the secret left blank, it is
  // kept. He then enables it.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const edit = card.getByRole('form', { name: 'Bot Teams' });
  await expect(edit.getByLabel('ID d’application (Microsoft App ID)')).toHaveCount(0);
  await expect(edit.getByLabel('ID de l’annuaire (locataire Microsoft Entra)')).toHaveCount(0);
  await edit.getByLabel('Nom', { exact: true }).fill(RENAMED);
  await edit.getByLabel('Secret client').fill(REFUSED_SECRET);
  await edit.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(edit.getByRole('alert')).toContainText('Microsoft Entra refuse l’application');
  expect((await storedTeamsBot(page)).gateway).toMatchObject({ name: NAME, secretsStored: true });
  await edit.getByLabel('Secret client').fill('');
  await edit.getByRole('button', { name: 'Enregistrer' }).click();
  const renamed = cardWith(page, RENAMED, 'Supprimer');
  await expect(renamed.getByText('0 compte(s) relié(s)', { exact: false })).toBeVisible();
  expect((await storedTeamsBot(page)).gateway).toMatchObject({
    name: RENAMED,
    workspaceId: TEAMS_DIRECTORY,
    botUserId: TEAMS_APP,
    secretsStored: true,
  });
  await renamed.getByRole('button', { name: 'Activer' }).click();
  await expect(renamed.getByText('Active', { exact: true })).toBeVisible();
  expect((await storedTeamsBot(page)).gateway?.enabled).toBe(true);
  // The secret kept is the one Entra takes: the bot answers Léa again.
  const offered = await botAnswer(request, endpoint, LEA_TEAMS, 'Bonjour');
  expect(offered).toContain('reliez votre compte Teams à votre compte');

  // Deleted: the endpoint answers as to an unknown gateway.
  await renamed.getByRole('button', { name: 'Supprimer' }).click();
  await expect(renamed).toHaveCount(0);
  expect(asked).toEqual([
    `Supprimer le bot « ${RENAMED} », son secret client et les 0 compte(s) Teams que les collaborateurs y ont relié(s) ?`,
  ]);
  expect((await storedTeamsBot(page)).gateway).toBeUndefined();
  await expect(section.getByRole('button', { name: 'Déclarer le bot Teams' })).toBeVisible();
  const gone = await sendActivity(request, endpoint, LEA_TEAMS, 'Bonjour');
  expect(gone.status()).toBe(404);
  await noError(page);
});

/** What `person`, signed in, is told of their identity on the gateway `id`. */
async function identityOn(person: Page, id: string | undefined) {
  const listed = await person.request.get('/api/account/gateways');
  expect(listed.status()).toBe(200);
  return ((await listed.json()) as AccountGateways).gateways.find((g) => g.gatewayId === id);
}
