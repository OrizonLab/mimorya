// The company's Telegram bot and the accounts people link there (ADR-133,
// ADR-134): Paul declares the bot from « Connecteurs » (its token and the
// webhook's secret never listed), Telegram is told where to send its
// updates. Léa asks for the bot's link in « Mon compte »; as the test
// Telegram sends them, she starts the bot with it, is told whose account
// it links and accepts it; back in « Mon compte », she sees which Telegram
// account accepted it and confirms. Paul sees her account counted and
// disables the bot; Léa is told so and removes her link; Paul renames the
// bot (its token kept), enables it and deletes it, after which Telegram
// sends its updates nowhere and the address answers as to an unknown
// gateway. Each step is read back from the API, each confirmation read.
// Whatever happens, the bot is deleted with its links. A link someone else
// accepted is refused, a lost link asked again, a link cancelled, none of
// them linking anything. The rules themselves are tested on the API
// (`crates/api/tests/telegram_gateway_*.rs`).
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { cardWith, openAdmin } from './support/admin';
import { deleteGateways } from './support/slack';
import {
  BOT_ID,
  BOT_USERNAME,
  LEA_TELEGRAM,
  MARC_TELEGRAM,
  TELEGRAM_TOKEN,
  botAnswer,
  codeOf,
  declareBot,
  pendingOf,
  sendUpdate,
  shownLink,
  storedBot,
  webhookOf,
} from './support/telegram';

const NAME = 'Telegram e2e';
const RENAMED = 'Bot de l’atelier e2e';

test.afterEach(async ({ page }) => deleteGateways(page));

test('a bot is declared, a Telegram account linked with both sides’ consent and removed, the bot deleted', async ({
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
  await section.getByRole('button', { name: 'Déclarer le bot Telegram' }).click();
  const form = section.getByRole('form', { name: 'Bot Telegram' });
  await form.getByLabel('Nom', { exact: true }).fill(NAME);
  await form.getByLabel('Jeton du bot').fill(TELEGRAM_TOKEN);
  await form.getByRole('button', { name: 'Déclarer le bot' }).click();
  const card = cardWith(page, NAME, 'Supprimer');
  await expect(card.getByText('Active', { exact: true })).toBeVisible();
  await expect(card.getByText(`Bot @${BOT_USERNAME}`, { exact: false })).toBeVisible();
  await expect(card.getByText('0 compte(s) relié(s)', { exact: false })).toBeVisible();
  // One per company: no second declaration is offered.
  await expect(section.getByRole('button', { name: 'Déclarer le bot Telegram' })).toHaveCount(0);
  await noError(page);
  const { gateway, listing } = await storedBot(page);
  expect(gateway).toMatchObject({
    name: NAME,
    enabled: true,
    workspaceId: String(BOT_ID),
    workspaceName: BOT_USERNAME,
    secretsStored: true,
    identities: 0,
  });
  // Telegram was told where, with a secret the platform made, never listed.
  const eventsUrl = gateway?.eventsUrl ?? '';
  expect(eventsUrl).toBe(`http://localhost:3000/api/gateways/telegram/${gateway?.id}`);
  await expect(card.getByText(eventsUrl, { exact: true })).toBeVisible();
  const webhook = await webhookOf(request);
  expect(webhook?.url).toBe(eventsUrl);
  expect(webhook?.secret).toMatch(/^[A-Za-z0-9_-]{64}$/);
  expect(listing).not.toContain(TELEGRAM_TOKEN);
  expect(listing).not.toContain(webhook?.secret ?? 'no secret');

  // Léa asks for the bot's link in « Mon compte ».
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
  await row.getByRole('button', { name: 'Relier mon compte Telegram' }).click();
  await expect(row.getByText('Liaison en cours', { exact: true })).toBeVisible();
  const code = codeOf(await shownLink(row));
  // The page alone knows the link: the API never lists the code.
  const { pending, listing: listedForLea } = await pendingOf(lea);
  expect(pending).toMatchObject({ claimedBy: null, claimedId: null });
  expect(listedForLea).not.toContain(code);
  await noError(lea);

  // She starts the bot with it: told whose account it links, she accepts.
  if (!webhook) throw new Error('a webhook');
  const offered = await botAnswer(request, webhook, LEA_TELEGRAM, `/start ${code}`);
  expect(offered).toContain('au compte de Léa Fontaine');
  const accepted = await botAnswer(request, webhook, LEA_TELEGRAM, '/oui');
  expect(accepted).toMatch(/^C’est noté/);

  // Back in « Mon compte », read again meanwhile: whom it came from, confirmed.
  const claimed = `@${LEA_TELEGRAM.username}`;
  await expect(row.getByText(claimed, { exact: true })).toBeVisible({ timeout: 10_000 });
  await expect(row.getByText(String(LEA_TELEGRAM.id), { exact: true })).toBeVisible();
  await row.getByRole('button', { name: 'Confirmer la liaison' }).click();
  await expect(row.getByText('Relié', { exact: true })).toBeVisible();
  await expect(
    row.getByText(`Votre compte Telegram ${claimed} (identifiant`, { exact: false }),
  ).toBeVisible();
  const mine = await lea.request.get('/api/account/gateways');
  expect(mine.status()).toBe(200);
  const listed = (await mine.json()) as { gateways: { type: string; linked: unknown }[] };
  expect(listed.gateways.find((g) => g.type === 'telegram')?.linked).toMatchObject({
    externalId: String(LEA_TELEGRAM.id),
    name: claimed,
    address: null,
  });
  // Writing to the bot, she is told to whose account hers is linked.
  const greeted = await botAnswer(request, webhook, LEA_TELEGRAM, 'Bonjour');
  expect(greeted).toContain('relié au compte de Léa Fontaine');
  await noError(lea);

  // Paul sees her account counted, and disables the bot.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await expect(card.getByText('1 compte(s) relié(s)', { exact: false })).toBeVisible();
  await card.getByRole('button', { name: 'Désactiver' }).click();
  await expect(card.getByText('Désactivée', { exact: true })).toBeVisible();
  expect((await storedBot(page)).gateway?.enabled).toBe(false);

  // Léa is told so in « Mon compte », and removes her link.
  await lea.reload();
  await expect(row.getByText(/^La passerelle est désactivée/)).toBeVisible();
  await row.getByRole('button', { name: 'Retirer la liaison' }).click();
  await expect(row.getByText('Non relié', { exact: true })).toBeVisible();
  await expect(row.getByText('La passerelle est indisponible pour l’instant.')).toBeVisible();
  expect(leaAsked).toEqual([
    `Retirer la liaison de votre compte Telegram au bot @${BOT_USERNAME} ? Vous pourrez le relier de nouveau d’ici.`,
  ]);
  await noError(lea);

  // Paul renames it, its token left blank and kept, then enables it.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const edit = card.getByRole('form', { name: 'Bot Telegram' });
  await edit.getByLabel('Nom', { exact: true }).fill(RENAMED);
  await edit.getByRole('button', { name: 'Enregistrer' }).click();
  const renamed = cardWith(page, RENAMED, 'Supprimer');
  await expect(renamed.getByText('0 compte(s) relié(s)', { exact: false })).toBeVisible();
  expect((await storedBot(page)).gateway).toMatchObject({ name: RENAMED, secretsStored: true });
  await renamed.getByRole('button', { name: 'Activer' }).click();
  await expect(renamed.getByText('Active', { exact: true })).toBeVisible();
  expect((await storedBot(page)).gateway?.enabled).toBe(true);

  // Deleted: Telegram sends its updates nowhere, the address answers as to
  // an unknown gateway.
  await renamed.getByRole('button', { name: 'Supprimer' }).click();
  await expect(renamed).toHaveCount(0);
  expect(asked).toEqual([
    `Supprimer le bot « ${RENAMED} », son jeton, son webhook et les 0 compte(s) Telegram que les collaborateurs y ont relié(s) ?`,
  ]);
  expect((await storedBot(page)).gateway).toBeUndefined();
  expect(await webhookOf(request)).toBeUndefined();
  await expect(section.getByRole('button', { name: 'Déclarer le bot Telegram' })).toBeVisible();
  const gone = await sendUpdate(request, webhook, LEA_TELEGRAM, 'Bonjour');
  expect(gone.status()).toBe(404);
  await noError(page);
});

test('a link someone else accepted is refused, a lost link asked again, a link cancelled', async ({
  page,
  request,
  newContext,
}) => {
  await signIn(page, demo.paul);
  const webhook = await declareBot(page, NAME);
  const lea = await (await newContext()).newPage();
  await signIn(lea, demo.lea);
  await lea.goto('/account');
  const row = lea.getByRole('region', { name: NAME });

  // Marc accepts her link, told it links Léa's account: she sees whom, and
  // refuses. The code is forgotten: it links nothing any more.
  await row.getByRole('button', { name: 'Relier mon compte Telegram' }).click();
  const first = codeOf(await shownLink(row));
  const offered = await botAnswer(request, webhook, MARC_TELEGRAM, `/start ${first}`);
  expect(offered).toContain('au compte de Léa Fontaine');
  await botAnswer(request, webhook, MARC_TELEGRAM, '/oui');
  const marc = `@${MARC_TELEGRAM.username}`;
  await expect(row.getByText(marc, { exact: true })).toBeVisible({ timeout: 10_000 });
  await row.getByRole('button', { name: 'Ce n’est pas moi' }).click();
  await expect(row.getByText('Non relié', { exact: true })).toBeVisible();
  expect((await pendingOf(lea)).pending).toBeNull();
  const again = await botAnswer(request, webhook, MARC_TELEGRAM, `/start ${first}`);
  expect(again).toContain('n’est plus valable');

  // A link lost with the page: a new one is asked, the former one linking
  // nothing.
  await row.getByRole('button', { name: 'Relier mon compte Telegram' }).click();
  const second = codeOf(await shownLink(row));
  await lea.reload();
  await expect(row.getByText('Vous n’avez plus le lien ? Demandez-en un nouveau.')).toBeVisible();
  await row.getByRole('button', { name: 'Demander un nouveau lien' }).click();
  const third = codeOf(await shownLink(row));
  expect(third).not.toBe(second);
  const former = await botAnswer(request, webhook, LEA_TELEGRAM, `/start ${second}`);
  expect(former).toContain('n’est plus valable');

  // Cancelled: nothing under way, the code links nothing.
  await row.getByRole('button', { name: 'Annuler la liaison' }).click();
  await expect(row.getByText('Non relié', { exact: true })).toBeVisible();
  expect((await pendingOf(lea)).pending).toBeNull();
  const cancelled = await botAnswer(request, webhook, LEA_TELEGRAM, `/start ${third}`);
  expect(cancelled).toContain('n’est plus valable');
  await noError(lea);
});
