// The company's Microsoft 365 application and the accounts its people link
// there (ADR-116, ADR-118): Paul declares the application (its secret never
// listed), is shown the return address to register in it, checks the
// declaration (Microsoft is not reached) and sets a tool's rule; Léa, from
// « Mon compte », links her account at the test provider, which plays
// Microsoft and comes back to her page; Paul sees her link counted and told
// in the deletion he refuses, renames the application and lets its tools
// write (ADR-120); Léa is asked to grant sending and writing her calendar,
// and links again; Paul disables the application; Léa is told her link no
// longer serves and removes it; Paul deletes the application. Each step is read back from the API, each confirmation read.
// The test server sends people to its provider instead of Microsoft; the
// tools are never called. Whatever happens, the application is deleted,
// with its links.
import type { Page } from '@playwright/test';
import type { AccountLinks, ConnectorView, Connectors } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const NAME = 'Microsoft 365 e2e';
const RENAMED = 'Messagerie e2e';
const DIRECTORY = '72f988bf-86f1-41af-91ab-2d7cd011db47';
/** The test provider's application, and its secret. */
const APPLICATION = '0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0';
const SECRET = 'secret-client-de-test-0123456789';
const WRITE = { 'X-Mimorya-Request': '1' };
const READ = 'm365_mail_read';
const WRITING = 'Permettre l’envoi d’e-mails et la création de rendez-vous, chacun validé';

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/admin/connectors');
  // 401: the test never signed in; 403: not as an administrator. Either
  // way, it declared nothing.
  if (listed.status() === 401 || listed.status() === 403) return;
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.type === 'microsoft365')) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

/** The company's application as the API records it, and the whole listing. */
async function stored(page: Page): Promise<{ app?: ConnectorView; listing: string }> {
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { instances } = JSON.parse(listing) as Connectors;
  return { app: instances.find((c) => c.type === 'microsoft365'), listing };
}

/** The person's link at the application, as the API lists it to them. */
async function linkOf(page: Page) {
  const listed = await page.request.get('/api/account/links');
  expect(listed.status()).toBe(200);
  const { connectors } = (await listed.json()) as AccountLinks;
  return connectors.find((c) => c.type === 'microsoft365');
}

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
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Déclarer l’application Microsoft 365' }).click();
  const form = page.getByRole('form', { name: 'Application Microsoft 365' });
  await form.getByLabel('Nom', { exact: true }).fill(NAME);
  await form.getByLabel('ID de l’annuaire (locataire)').fill(DIRECTORY);
  await form.getByLabel('ID de l’application (client)').fill(APPLICATION);
  await form.getByLabel('Secret de l’application').fill(SECRET);
  await form.getByRole('button', { name: 'Déclarer l’application' }).click();
  const card = cardWith(page, NAME, 'Supprimer');
  await expect(card.getByText('0 compte(s) relié(s)')).toBeVisible();
  await expect(card.getByText(/^Lecture seule/)).toBeVisible();
  await expect(card.getByText('/api/account/links/callback')).toBeVisible();
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
  const back = lea.waitForResponse((r) => r.url().includes('/api/account/links/callback'));
  await section.getByRole('button', { name: 'Relier mon compte' }).click();
  await back;
  await expect(lea.getByText('Votre compte Microsoft 365 est relié')).toBeVisible();
  await expect(section.getByText(`Relié à ${demo.lea}`)).toBeVisible();
  await expect(
    section.getByText(/L’agent peut : lire vos e-mails, lire votre agenda\./),
  ).toBeVisible();
  await expect(section.getByRole('button', { name: 'Relier de nouveau' })).toBeVisible();
  await noError(lea);
  expect((await linkOf(lea))?.linked?.address).toBe(demo.lea);
  // Said once: the page reloaded no longer says it.
  await lea.reload();
  await expect(section.getByText(`Relié à ${demo.lea}`)).toBeVisible();
  await expect(lea.getByText('Votre compte Microsoft 365 est relié')).toHaveCount(0);

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

  // Renamed, its tools let write: Léa is asked to grant it, and links again.
  await card.getByRole('button', { name: 'Modifier', exact: true }).click();
  const edit = page.getByRole('form', { name: 'Application Microsoft 365' });
  await edit.getByLabel('Nom', { exact: true }).fill(RENAMED);
  await edit.getByRole('checkbox', { name: WRITING }).check();
  await edit.getByRole('button', { name: 'Enregistrer' }).click();
  const renamedCard = cardWith(page, RENAMED, 'Supprimer');
  await expect(
    renamedCard.getByText(/^Envoi d’e-mails et création de rendez-vous permis/),
  ).toBeVisible();
  expect((await stored(page)).app?.name).toBe(RENAMED);
  expect((await stored(page)).app?.microsoft365?.writeTools).toBe(true);
  await lea.reload();
  const renamed = lea.getByRole('region', { name: RENAMED });
  const asking = renamed.getByText(
    /demande aussi votre accord pour : envoyer des e-mails en votre nom, écrire dans votre agenda\./,
  );
  await expect(asking).toBeVisible();
  expect((await linkOf(lea))?.linked?.toGrant).toEqual(['Mail.Send', 'Calendars.ReadWrite']);
  const again = lea.waitForResponse((r) => r.url().includes('/api/account/links/callback'));
  await renamed.getByRole('button', { name: 'Relier de nouveau' }).click();
  await again;
  await expect(
    renamed.getByText(
      /L’agent peut : .*envoyer des e-mails en votre nom, écrire dans votre agenda\./,
    ),
  ).toBeVisible();
  await expect(asking).toHaveCount(0);
  expect((await linkOf(lea))?.linked?.toGrant).toEqual([]);

  // Disabled: Léa's link no longer serves, she may remove it.
  await renamedCard.getByRole('button', { name: 'Désactiver' }).click();
  await expect.poll(async () => (await stored(page)).app?.enabled).toBe(false);
  await lea.reload();
  await expect(renamed.getByText(/L’application est désactivée/)).toBeVisible();
  await expect(renamed.getByRole('button', { name: /^Relier/ })).toHaveCount(0);
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
