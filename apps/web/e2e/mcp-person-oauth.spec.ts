// An MCP server each person authorizes for themself (ADR-174), from end to
// end:
// - the administration declares it so and sees how many people did, never
//   who; its test goes with the administrator's own authorization alone;
// - a run of Marc's, who has not authorized it, tells him so, with the way
//   to « Mon compte », where he authorizes it there and back with the
//   authorization server, then withdraws it;
// - who authorizes it is changed back to the company, every person's
//   authorization taken, then to each person again, the company's taken.
// A return that failed is said in « Mon compte », once; a renewal that
// failed is said to whose authorization it is (Lucas's, written by the
// end-to-end API at its start, `person_renewal.rs`).
// The end-to-end API serves the server and its authorization server
// (`e2e_server`). The server added is deleted, and a validation left
// waiting is refused, even when the test fails.
import type { Page } from '@playwright/test';
import type { PersonAuthorizations } from '../lib/api/contract';
import {
  addServer,
  cardWith,
  connectorsTab,
  deleteConnectorsNamed,
  openAdmin,
} from './support/admin';
import { refuseWaiting } from './support/approvals';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import {
  OAUTH_CLIENT_ID as CLIENT_ID,
  OAUTH_CLIENT_SECRET as CLIENT_SECRET,
  OAUTH_SERVER as ADDRESS,
  connectorNamed,
} from './support/oauth';
import { REFERENCE_REQUEST } from './support/scenario';

const SERVER = 'Fichiers de chacun e2e';
/** Lucas's, its last renewal failed: written by the end-to-end API. */
const TO_RENEW = 'Serveur à renouveler';

/** The server as the API lists it to its administrators. */
async function server(page: Page) {
  return connectorNamed(page, SERVER);
}

/** The confirmations asked so far in the test, in order, on any page. */
let confirmations: string[] = [];

function confirming(page: Page) {
  page.on('dialog', (dialog) => {
    confirmations.push(dialog.message());
    void dialog.accept();
  });
}

test.beforeEach(({ page }) => {
  confirmations = [];
  confirming(page);
});

test.afterEach(async ({ page, newContext }) => {
  const marc = await (await newContext()).newPage();
  await signIn(marc, demo.marc);
  await refuseWaiting(marc);
  // 401 or 403: the test never signed in, or not as an administrator, so
  // added nothing; any other failure is said.
  await deleteConnectorsNamed(page, [SERVER], [401, 403]);
});

test('each person authorizes a server for themself, told to by their run', async ({
  page,
  newContext,
}) => {
  await openAdmin(page, 'Connecteurs');
  await addServer(page, SERVER, 'chacun', ADDRESS);

  // Declared for each person: none has authorized it, nothing to authorize
  // for the company here.
  const card = cardWith(page, SERVER, 'Supprimer');
  await card.getByRole('button', { name: 'Déclarer OAuth' }).click();
  await card.getByLabel('Identifiant du client').fill(CLIENT_ID);
  await card.getByLabel(/^Secret du client/).fill(CLIENT_SECRET);
  await card.getByLabel('Chaque personne, pour elle-même, dans « Mon compte »').check();
  await card.getByRole('button', { name: 'Déclarer', exact: true }).click();
  await expect(card.getByText('Chaque personne l’autorise', { exact: true })).toBeVisible();
  await expect(
    card.getByText('Aucune personne ne l’a encore autorisé pour elle-même.'),
  ).toBeVisible();
  await expect(card.getByText(/· OAuth, autorisé par chaque personne$/)).toBeVisible();
  await expect(card.getByRole('button', { name: 'Autoriser', exact: true })).toHaveCount(0);
  const declared = await server(page);
  expect(declared?.mcp?.oauth?.perPerson).toBe(true);
  expect(declared?.mcp?.oauth?.personAuthorizations).toBe(0);
  // Its test goes with the administrator's own authorization: none yet.
  await card.getByRole('button', { name: 'Tester la connexion' }).click();
  await expect(card.getByText(/autorisez-le d’abord dans « Mon compte »/)).toBeVisible();

  // Marc's run goes without it, and tells him where to authorize it.
  const marc = await (await newContext()).newPage();
  confirming(marc);
  await signIn(marc, demo.marc);
  await marc.getByLabel('Votre message').fill(REFERENCE_REQUEST);
  await marc.getByRole('button', { name: 'Envoyer' }).click();
  await expect(
    marc.getByText(
      `Le connecteur « ${SERVER} » s’utilise avec votre propre autorisation, que vous n’avez pas donnée : la réponse peut être incomplète.`,
    ),
  ).toBeVisible({ timeout: 30_000 });
  // While it runs, the way to his account; it goes on with the other
  // connectors, its validation refused.
  const way = marc.getByRole('link', { name: 'L’autoriser dans « Mon compte »' });
  await expect(way).toHaveAttribute('href', '/account');
  await expect(marc.getByText('Validation requise')).toBeVisible({ timeout: 30_000 });
  await marc.getByRole('button', { name: 'Refuser' }).click();
  await expect(
    marc.getByText('Le planning est inchangé : la validation a été refusée.'),
  ).toBeVisible({ timeout: 30_000 });

  // There and back from his account, through the sidebar (the notice's
  // link may still be shown): authorized, said once.
  await marc.getByRole('link', { name: /Mon compte ·/ }).click();
  await expect(marc).toHaveURL(/\/account$/);
  const row = marc.getByRole('region', { name: SERVER });
  await expect(row.getByText('Non autorisé', { exact: true })).toBeVisible();
  await row.getByRole('button', { name: 'Autoriser', exact: true }).click();
  await expect(
    marc.getByText(/^Serveur autorisé : l’agent l’utilise désormais pour vous/),
  ).toBeVisible();
  await expect(marc).toHaveURL(/\/account$/);
  await expect(row.getByText('Autorisé', { exact: true })).toBeVisible();
  await expect(row.getByText(/^Autorisé le .* · renouvelé avant son échéance$/)).toBeVisible();
  const his = await marc.request.get('/api/account/connector-authorizations');
  expect(his.status()).toBe(200);
  const mine = ((await his.json()) as PersonAuthorizations).servers.find((s) => s.name === SERVER);
  expect(mine?.available).toBe(true);
  expect(mine?.authorization?.renewable).toBe(true);
  expect(mine?.authorization?.renewalFailure).toBeNull();
  expect(JSON.stringify(mine)).not.toContain(CLIENT_SECRET);

  // Paul authorizes it for himself: his test then goes with his own; two
  // people counted, never named.
  await page.goto('/account');
  const pauls = page.getByRole('region', { name: SERVER });
  await pauls.getByRole('button', { name: 'Autoriser', exact: true }).click();
  await expect(page.getByText(/^Serveur autorisé : /)).toBeVisible();
  await connectorsTab(page);
  const card2 = cardWith(page, SERVER, 'Supprimer');
  await expect(card2.getByText('2 personnes l’ont autorisé pour elles-mêmes.')).toBeVisible();
  await card2.getByRole('button', { name: 'Tester la connexion' }).click();
  await expect(
    card2.getByText(/^Connexion réussie : [1-9]\d* outil\(s\) exposé\(s\)/),
  ).toBeVisible();
  expect(JSON.stringify(await server(page))).not.toContain(demo.marc);

  // Marc withdraws his: no longer authorized, its tokens revoked.
  await marc
    .getByRole('region', { name: SERVER })
    .getByRole('button', {
      name: 'Retirer mon autorisation',
    })
    .click();
  await expect(row.getByText('Non autorisé', { exact: true })).toBeVisible();
  expect(confirmations.at(-1)).toContain('révoquer vos jetons');
  await noError(marc);

  // Back to the company, Paul's own the last one taken: the company's to
  // give.
  await connectorsTab(page);
  await expect(card2.getByText('1 personne l’a autorisé pour elle-même.')).toBeVisible();
  await card2.getByRole('button', { name: 'Faire autoriser par l’entreprise' }).click();
  expect(confirmations.at(-1)).toContain('Les autorisations des personnes (1) sont retirées');
  await expect(card2.getByText('Non autorisé', { exact: true })).toBeVisible();
  await expect(card2.getByRole('button', { name: 'Autoriser', exact: true })).toBeVisible();
  await expect(card2.getByText(/· OAuth déclaré, non autorisé$/)).toBeVisible();
  const company = await server(page);
  expect(company?.mcp?.oauth?.perPerson).toBe(false);
  expect(company?.mcp?.oauth?.personAuthorizations).toBe(0);

  // Authorized for the company, then each person's again: the company's
  // taken, its tokens revoked.
  await card2.getByRole('button', { name: 'Autoriser', exact: true }).click();
  await expect(page.getByText('Serveur MCP autorisé')).toBeVisible();
  await expect(card2.getByText(/^Autorisé le /)).toBeVisible();
  await card2.getByRole('button', { name: 'Faire autoriser par chaque personne' }).click();
  expect(confirmations.at(-1)).toContain(
    'L’autorisation de l’entreprise est retirée ; la plateforme demande au serveur d’autorisation de révoquer ses jetons.',
  );
  await expect(card2.getByText('Chaque personne l’autorise', { exact: true })).toBeVisible();
  const each = await server(page);
  expect(each?.mcp?.oauth?.perPerson).toBe(true);
  expect(each?.mcp?.oauth?.authorization).toBeNull();
  await noError(page);
});

test('a failed return is said in « Mon compte », once', async ({ page }) => {
  await signIn(page, demo.marc);
  await page.goto('/account?serveur=echec');
  const said = page.getByText(/^L’autorisation a échoué : rien n’a été gardé\./);
  await expect(said).toBeVisible();
  await expect(page).toHaveURL(/\/account$/);
  await page.reload();
  // Its sections loaded, so that what is absent is truly absent.
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  await expect(page.getByText('Serveurs que vous autorisez')).toHaveCount(0);
  await expect(said).toHaveCount(0);
});

test('a renewal that failed is said to whose authorization it is', async ({ page, newContext }) => {
  await signIn(page, demo.lucas);
  await page.goto('/account');
  const row = page.getByRole('region', { name: TO_RENEW });
  await expect(row.getByText('Autorisé', { exact: true })).toBeVisible();
  await expect(
    row.getByText(
      'Dernier renouvellement échoué : Serveur d’autorisation injoignable (délai dépassé)',
    ),
  ).toBeVisible();
  // Disabled: no longer his to authorize, his to withdraw.
  await expect(row.getByText(/^Ce serveur n’est plus disponible pour vous/)).toBeVisible();
  await expect(row.getByRole('button', { name: /^Autoriser/ })).toHaveCount(0);
  await expect(row.getByRole('button', { name: 'Retirer mon autorisation' })).toBeVisible();
  // His alone: nothing of it in Marc's account.
  const marc = await (await newContext()).newPage();
  await signIn(marc, demo.marc);
  const his = await marc.request.get('/api/account/connector-authorizations');
  expect(his.status()).toBe(200);
  const servers = ((await his.json()) as PersonAuthorizations).servers;
  expect(servers.map((server) => server.name)).not.toContain(TO_RENEW);
});
