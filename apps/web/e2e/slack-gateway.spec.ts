// The company's Slack gateway and the identities people link there
// (ADR-125, ADR-126): Paul declares the application from « Connecteurs »
// (its token and secret never listed) and is shown the address to give
// Slack; Léa is told in « Mon compte » how to link her Slack account; she
// writes to the application, as the test Slack sends it, signed, and the
// application answers with a link. Opened signed out, the link asks to
// sign in first; opened in her session, it shows what it links and links
// it. Paul sees her identity counted and disables the gateway; the link,
// reopened, says why it links nothing; Léa is told so in « Mon compte » and
// removes her link; Paul renames the gateway (its token and secret
// kept), enables it and deletes it, after which the address answers as to
// an unknown gateway. Each step is read back from the API, each
// confirmation read. Whatever happens, the gateway is deleted with its
// identities. The rules themselves are tested on the API
// (`crates/api/tests/slack_gateway_*.rs`).
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { cardWith, openAdmin } from './support/admin';
import {
  BOT_TOKEN,
  LEA_SLACK,
  SIGNING_SECRET,
  WORKSPACE,
  WORKSPACE_NAME,
  deleteGateways,
  offeredLink,
  posts,
  sendDirectMessage,
  storedGateway,
} from './support/slack';

const NAME = 'Slack e2e';
const RENAMED = 'Messagerie Slack e2e';
const LINKED = 'Votre compte Slack est relié à votre compte.';

test.afterEach(async ({ page }) => deleteGateways(page));

test('a gateway is declared, a Slack identity linked from its link and removed, the gateway deleted', async ({
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
  await section.getByRole('button', { name: 'Déclarer l’application Slack' }).click();
  const form = section.getByRole('form', { name: 'Application Slack' });
  await form.getByLabel('Nom', { exact: true }).fill(NAME);
  await form.getByLabel('Jeton du bot').fill(BOT_TOKEN);
  await form.getByLabel('Secret de signature').fill(SIGNING_SECRET);
  await form.getByRole('button', { name: 'Déclarer l’application' }).click();
  const card = cardWith(page, NAME, 'Supprimer');
  await expect(card.getByText('Active', { exact: true })).toBeVisible();
  await expect(card.getByText('0 compte(s) relié(s)', { exact: false })).toBeVisible();
  await expect(card.getByText(`Espace de travail « ${WORKSPACE_NAME} »`)).toBeVisible();
  // One per company: no second declaration is offered.
  await expect(section.getByRole('button', { name: 'Déclarer l’application Slack' })).toHaveCount(
    0,
  );
  await noError(page);
  const { gateway, listing } = await storedGateway(page);
  expect(gateway).toMatchObject({
    name: NAME,
    enabled: true,
    workspaceId: WORKSPACE,
    workspaceName: WORKSPACE_NAME,
    botUserId: 'U0MIMORYA',
    secretsStored: true,
    identities: 0,
  });
  expect(listing).not.toContain(BOT_TOKEN);
  expect(listing).not.toContain(SIGNING_SECRET);
  const eventsUrl = gateway?.eventsUrl ?? '';
  expect(eventsUrl).toBe(`http://localhost:3000/api/gateways/slack/${gateway?.id}`);
  await expect(card.getByText(eventsUrl, { exact: true })).toBeVisible();
  // The same address for the validation buttons (ADR-129).
  await expect(card.getByText(/« Interactivity & Shortcuts »/)).toBeVisible();

  // Léa is told how to link her Slack account.
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
    row.getByText(`écrivez à l’application de l’entreprise dans l’espace « ${WORKSPACE_NAME} »`, {
      exact: false,
    }),
  ).toBeVisible();

  // She writes to the application: it answers with a link to her page.
  const before = (await posts(request)).length;
  const sent = await sendDirectMessage(request, eventsUrl, LEA_SLACK, 'Bonjour');
  expect(sent.status()).toBe(200);
  const link = await offeredLink(request, before);
  expect(link).toMatch(/^http:\/\/localhost:3000\/account\/gateway-link#.+/);

  // Opened signed out, it asks to sign in first.
  const outside = await (await newContext()).newPage();
  await outside.goto(link);
  await expect(outside.getByText(/^Connectez-vous d’abord, puis rouvrez ce lien/)).toBeVisible();
  await expect(outside.getByRole('link', { name: 'Se connecter' })).toBeVisible();

  // Opened in her session, it shows what it links, then links it.
  await lea.goto(link);
  await expect(lea.getByText(LEA_SLACK, { exact: true })).toBeVisible();
  await expect(lea.getByText(`« ${WORKSPACE_NAME} » (${NAME})`, { exact: false })).toBeVisible();
  await lea.getByRole('button', { name: 'Relier mon compte Slack' }).click();
  await expect(lea.getByRole('status')).toHaveText(LINKED);
  await noError(lea);
  const linked = await lea.request.get('/api/account/gateways');
  expect(linked.status()).toBe(200);
  const mine = ((await linked.json()) as { gateways: { linked: unknown }[] }).gateways[0];
  expect(mine?.linked).toMatchObject({ externalId: LEA_SLACK, address: demo.lea });
  await lea.getByRole('link', { name: 'Mon compte' }).click();
  await expect(row.getByText('Relié', { exact: true })).toBeVisible();
  await expect(row.getByText(`(${demo.lea}) de l’espace`, { exact: false })).toBeVisible();
  // There, the requests awaiting her validation are announced (ADR-130).
  await expect(
    row.getByText(/vous y annonce les demandes attendant votre validation\.$/),
  ).toBeVisible();

  // Paul sees her identity counted, and disables the gateway.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await expect(card.getByText('1 compte(s) relié(s)', { exact: false })).toBeVisible();
  await card.getByRole('button', { name: 'Désactiver' }).click();
  await expect(card.getByText('Désactivée', { exact: true })).toBeVisible();
  expect((await storedGateway(page)).gateway?.enabled).toBe(false);

  // The link, reopened, says why it links nothing.
  await lea.goto(link);
  await expect(
    lea.getByText(/^La passerelle Slack de votre entreprise est désactivée/),
  ).toBeVisible();
  await expect(lea.getByRole('button', { name: 'Relier mon compte Slack' })).toHaveCount(0);

  // Léa is told so in « Mon compte », and removes her link.
  await lea.goto('/account');
  await expect(row.getByText(/^La passerelle est désactivée/)).toBeVisible();
  await row.getByRole('button', { name: 'Retirer la liaison' }).click();
  await expect(row.getByText('Non relié', { exact: true })).toBeVisible();
  await expect(row.getByText('La passerelle est indisponible pour l’instant.')).toBeVisible();
  expect(leaAsked).toEqual([
    `Retirer la liaison de votre compte Slack de l’espace « ${WORKSPACE_NAME} » ? Vous pourrez le relier de nouveau depuis le lien que l’application vous enverra.`,
  ]);
  const unlinked = await lea.request.get('/api/account/gateways');
  expect(((await unlinked.json()) as { gateways: { linked: unknown }[] }).gateways[0]?.linked).toBe(
    null,
  );
  await noError(lea);

  // Paul renames it, its token and secret left blank and kept, then enables it.
  await page.reload();
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const edit = card.getByRole('form', { name: 'Application Slack' });
  // Managers are told there of the requests awaiting them (ADR-130).
  await expect(
    edit.getByText(/Un responsable relié, ou qui le remplace, y est aussi/),
  ).toBeVisible();
  await edit.getByLabel('Nom', { exact: true }).fill(RENAMED);
  await edit.getByRole('button', { name: 'Enregistrer' }).click();
  const renamed = cardWith(page, RENAMED, 'Supprimer');
  await expect(renamed.getByText('0 compte(s) relié(s)', { exact: false })).toBeVisible();
  expect((await storedGateway(page)).gateway).toMatchObject({
    name: RENAMED,
    secretsStored: true,
  });
  await renamed.getByRole('button', { name: 'Activer' }).click();
  await expect(renamed.getByText('Active', { exact: true })).toBeVisible();
  expect((await storedGateway(page)).gateway?.enabled).toBe(true);

  // Deleted: the address answers as to an unknown gateway.
  await renamed.getByRole('button', { name: 'Supprimer' }).click();
  await expect(renamed).toHaveCount(0);
  expect(asked).toEqual([
    `Supprimer la passerelle « ${RENAMED} », son jeton, son secret et les 0 compte(s) Slack que les collaborateurs y ont relié(s) ?`,
  ]);
  expect((await storedGateway(page)).gateway).toBeUndefined();
  await expect(section.getByRole('button', { name: 'Déclarer l’application Slack' })).toBeVisible();
  const gone = await sendDirectMessage(request, eventsUrl, LEA_SLACK, 'Bonjour');
  expect(gone.status()).toBe(404);
  await noError(page);
});
