// The permissions of the company's Slack application (ADR-132), as the
// test Slack says them with its token: Paul declares the application with
// a token lacking im:read and is told what to add, nothing declared; given
// it, the card says every permission asked for is granted. The application
// reinstalled with one less and one more, he checks it again from the card,
// which names both; given back, checked again, nothing is missing nor
// more; Slack saying none, they are shown unverified. Each step read back
// from the API. Whatever happens, the test Slack
// gives the token back the permissions asked for and the gateway is
// deleted. The rules themselves are tested on the API
// (`crates/api/tests/slack_gateway_scopes.rs`).
import { expect, noError, test } from './support/fixtures';
import { cardWith, openAdmin } from './support/admin';
import {
  BOT_TOKEN,
  SIGNING_SECRET,
  deleteGateways,
  grantScopes,
  storedGateway,
} from './support/slack';

const NAME = 'Slack permissions e2e';
const ASKED = ['chat:write', 'im:history', 'im:read', 'users:read', 'users:read.email'];
const WITHOUT_IM_READ = ASKED.filter((scope) => scope !== 'im:read');
const CHECK = 'Vérifier auprès de Slack';
const ALL_GRANTED = /^Toutes les permissions demandées sont accordées/;

test.afterEach(async ({ page, request }) => {
  await grantScopes(request, ASKED);
  await deleteGateways(page);
});

test('a token lacking a permission is refused; a gateway checked again names what is missing and more', async ({
  page,
  request,
}) => {
  await grantScopes(request, WITHOUT_IM_READ);
  await openAdmin(page, 'Connecteurs');
  const section = page.getByRole('region', { name: 'Passerelles de conversation' });
  await section.getByRole('button', { name: 'Déclarer l’application Slack' }).click();
  const form = section.getByRole('form', { name: 'Application Slack' });
  await form.getByLabel('Nom', { exact: true }).fill(NAME);
  await form.getByLabel('Jeton du bot').fill(BOT_TOKEN);
  await form.getByLabel('Secret de signature').fill(SIGNING_SECRET);
  await form.getByRole('button', { name: 'Déclarer l’application' }).click();
  await expect(
    form.getByText(/^Il manque à l’application Slack la permission im:read : ajoutez-la/),
  ).toBeVisible();
  expect((await storedGateway(page)).gateway).toBeUndefined();

  // Given im:read: declared, every permission granted.
  await grantScopes(request, ASKED);
  await form.getByRole('button', { name: 'Déclarer l’application' }).click();
  const card = cardWith(page, NAME, CHECK);
  await expect(card.getByText(ALL_GRANTED)).toBeVisible();
  expect((await storedGateway(page)).gateway).toMatchObject({
    name: NAME,
    missingScopes: [],
    extraScopes: [],
  });

  // Reinstalled with one less and one more: checked again, both named.
  await grantScopes(request, [...WITHOUT_IM_READ, 'channels:history']);
  await card.getByRole('button', { name: CHECK }).click();
  await expect(card.getByText(/^Permissions manquantes : im:read\./)).toBeVisible();
  await expect(
    card.getByText(/^Permissions en plus de celles demandées : channels:history\./),
  ).toBeVisible();
  expect((await storedGateway(page)).gateway).toMatchObject({
    missingScopes: ['im:read'],
    extraScopes: ['channels:history'],
  });

  // Given back: nothing missing nor more.
  await grantScopes(request, ASKED);
  await card.getByRole('button', { name: CHECK }).click();
  await expect(card.getByText(ALL_GRANTED)).toBeVisible();
  await expect(card.getByText(/^Permissions en plus/)).toHaveCount(0);
  expect((await storedGateway(page)).gateway).toMatchObject({
    missingScopes: [],
    extraScopes: [],
  });

  // Slack saying none: unverified.
  await grantScopes(request, null);
  await card.getByRole('button', { name: CHECK }).click();
  await expect(
    card.getByText(/^Permissions du jeton non vérifiées : Slack ne les a pas dites le /),
  ).toBeVisible();
  expect((await storedGateway(page)).gateway).toMatchObject({
    missingScopes: null,
    extraScopes: null,
  });
  await noError(page);
});
