// A person's account at the company's standard mail (ADR-205), linked from
// « Mon compte » against the end-to-end API's simulated IMAP, SMTP and
// CalDAV servers: Paul declares the mail; Léa is shown where her password
// goes, a wrong password is refused and nothing is linked, the right one
// links her account (read back from the API, the password never in it);
// Paul's card counts it; she removes it, confirming first. Then the
// servers it was checked against: Paul, his tab loaded before she linked
// it, moves the IMAP server to another host; the server refuses it,
// counting her account, and he is asked; he declines, and nothing changes;
// SMTP secured otherwise moves no host, asks nothing and sends only what
// changed; he removes the agenda and accepts, the change sent again with
// the account he agreed to let go, and hers is. Last, a mail the company
// disabled: she is told, and may only remove the account she kept.
// Whatever happens, the mail is deleted, with her account.
import type { Page } from '@playwright/test';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { cardWith, connectorsTab } from './support/admin';
import {
  LEA_MAIL,
  TEST_SERVERS,
  declareOnTestServers,
  deleteMailServers,
  stored,
} from './support/mail-server';
import type { AccountMail, ConnectorUpdate, MailAccountLink } from '../lib/api/contract';

const NAME = 'Messagerie e2e';

test.afterEach(async ({ page }) => deleteMailServers(page));

test('an account is linked, refused with a wrong password, then removed', async ({
  page,
  newContext,
}) => {
  await signIn(page, demo.paul);
  await declareOnTestServers(page, NAME);

  const lea = await (await newContext()).newPage();
  const asked: string[] = [];
  lea.on('dialog', (dialog) => {
    asked.push(dialog.message());
    void dialog.accept();
  });
  await signIn(lea, demo.lea);
  await lea.goto('/account');
  const section = lea.getByRole('region', { name: NAME });
  await expect(section.getByText('Non relié')).toBeVisible();
  // Where her password goes, each host once.
  await expect(
    section.getByText(/^Votre mot de passe part vers 127\.0\.0\.1, toujours/),
  ).toBeVisible();

  const form = section.getByRole('form', { name: 'Compte de messagerie' });
  await form.getByLabel('Identifiant de messagerie').fill(LEA_MAIL.username);
  await form.getByLabel('Mot de passe de messagerie').fill('Pas-le-bon-1!');
  await form.getByRole('button', { name: 'Relier mon compte' }).click();
  await expect(
    section.getByText(/^Serveur IMAP : Identifiant ou mot de passe refusé/),
  ).toBeVisible();
  // The password typed leaves the page.
  await expect(form.getByLabel('Mot de passe de messagerie')).toHaveValue('');
  let read = (await (await lea.request.get('/api/account/mail')).json()) as AccountMail;
  expect(read.mail?.linked).toBeNull();

  await form.getByLabel('Mot de passe de messagerie').fill(LEA_MAIL.password);
  await form.getByRole('button', { name: 'Relier mon compte' }).click();
  await expect(section.getByRole('status')).toHaveText(/^Votre compte est relié/);
  await expect(section.getByText('Relié', { exact: true })).toBeVisible();
  await expect(section.getByText(`Compte ${LEA_MAIL.username}, relié le`)).toBeVisible();
  await noError(lea);
  const answer = await lea.request.get('/api/account/mail');
  const listing = await answer.text();
  read = JSON.parse(listing) as AccountMail;
  expect(read.mail?.linked?.username).toBe(LEA_MAIL.username);
  expect(listing).not.toContain(LEA_MAIL.password);

  // The administrator sees one account linked, never whose password.
  await connectorsTab(page);
  await expect(page.getByText(/· 1 compte\(s\) relié\(s\)$/)).toBeVisible();
  expect((await stored(page))?.mailServer?.accounts).toBe(1);

  await form.getByRole('button', { name: 'Retirer mon compte' }).click();
  await expect(section.getByText('Non relié')).toBeVisible();
  expect(asked).toEqual([
    `Retirer votre compte de « ${NAME} » ? Son mot de passe chiffré est effacé : l’agent ne lit plus votre boîte ni votre agenda.`,
  ]);
  read = (await (await lea.request.get('/api/account/mail')).json()) as AccountMail;
  expect(read.mail?.linked).toBeNull();
  expect((await stored(page))?.mailServer?.accounts).toBe(0);
});

/** Links Léa's account through the API, as « Mon compte » does. */
async function linkLea(lea: Page) {
  const linked = await lea.request.put('/api/account/mail', {
    headers: { 'X-Mimorya-Request': '1' },
    data: {
      username: LEA_MAIL.username,
      password: LEA_MAIL.password,
    } satisfies MailAccountLink,
  });
  expect(linked.status(), await linked.text()).toBe(200);
}

test('moving a server asks first, then lets go of the accounts linked there', async ({
  page,
  newContext,
}) => {
  await signIn(page, demo.paul);
  await declareOnTestServers(page, NAME);
  // His tab counts no account: Léa links hers after it loaded.
  await connectorsTab(page);
  const card = cardWith(page, NAME, 'Supprimer');
  await expect(card.getByText(/· 0 compte\(s\) relié\(s\)$/)).toBeVisible();
  const lea = await (await newContext()).newPage();
  await signIn(lea, demo.lea);
  await linkLea(lea);

  const asked: string[] = [];
  let accept = false;
  page.on('dialog', (dialog) => {
    asked.push(dialog.message());
    void (accept ? dialog.accept() : dialog.dismiss());
  });
  // Each change sent, and what the server answered.
  const patches: { body: unknown; status: number }[] = [];
  page.on('response', (response) => {
    const request = response.request();
    if (request.method() !== 'PATCH') return;
    patches.push({ body: request.postDataJSON() as unknown, status: response.status() });
  });
  const question =
    'Les serveurs où partent les mots de passe changent : les 1 compte(s) relié(s) sont retirés, et chacun relie le sien de nouveau dans « Mon compte ». Continuer ?';

  // Another host, declined: refused by the server, which counts the
  // account linked since; asked; nothing sent again, nothing changed.
  await card.getByRole('button', { name: 'Modifier' }).click();
  const form = card.getByRole('form', { name: 'Messagerie standard' });
  const imapHost = form.getByLabel('Serveur IMAP (TLS dès la connexion)');
  await imapHost.fill('imap.neo-maintenance.example');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect.poll(() => asked).toEqual([question]);
  const moved = { host: 'imap.neo-maintenance.example', port: TEST_SERVERS.imap.port };
  await expect.poll(() => patches).toEqual([{ body: { name: NAME, imap: moved }, status: 409 }]);
  await expect(form.getByRole('button', { name: 'Enregistrer' })).toBeEnabled();
  await expect(form.getByRole('alert')).toHaveCount(0);
  let mail = (await stored(page))?.mailServer;
  expect(mail?.imap).toEqual(TEST_SERVERS.imap);
  expect(mail?.accounts).toBe(1);

  // SMTP secured otherwise moves no host: nothing asked, only what
  // changed sent (its port, not the usual one, stays).
  await imapHost.fill(TEST_SERVERS.imap.host);
  await form.getByLabel('Sécurité SMTP').selectOption('tls');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(
    card.getByText(/ · SMTP 127\.0\.0\.1:4113 \(TLS dès la connexion\) · /),
  ).toBeVisible();
  expect(asked).toEqual([question]);
  const secured = { ...TEST_SERVERS.smtp, security: 'tls' };
  expect(patches.slice(1)).toEqual([{ body: { name: NAME, smtp: secured }, status: 200 }]);
  mail = (await stored(page))?.mailServer;
  expect(mail?.imap).toEqual(TEST_SERVERS.imap);
  expect(mail?.smtp).toEqual(secured);
  expect(mail?.caldav).toBe(TEST_SERVERS.caldav);
  expect(mail?.accounts).toBe(1);

  // The agenda removed is another host gone, accepted: sent again with
  // the account he agreed to let go, and hers is. While that sending is
  // held, the form stays busy: nothing to send twice, nothing to close.
  accept = true;
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/api/admin/connectors/*', async (route) => {
    const body = route.request().postDataJSON() as ConnectorUpdate | null;
    if (body?.mailAccountsLetGo !== undefined) await held;
    await route.fallback();
  });
  await card.getByRole('button', { name: 'Modifier' }).click();
  await form.getByLabel('Agenda CalDAV (facultatif, adresse en https://)').fill('');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect.poll(() => asked.length).toBe(2);
  await expect(form.getByRole('button', { name: 'Enregistrer' })).toBeDisabled();
  await expect(form.getByRole('button', { name: 'Annuler' })).toBeDisabled();
  release();
  await expect(card.getByText(/· 0 compte\(s\) relié\(s\)$/)).toBeVisible();
  await expect(card.getByText('Aucun agenda déclaré')).toBeVisible();
  expect(asked).toEqual([question, question]);
  expect(patches.slice(2)).toEqual([
    { body: { name: NAME, caldav: null }, status: 409 },
    { body: { name: NAME, caldav: null, mailAccountsLetGo: 1 }, status: 200 },
  ]);
  await noError(page);
  mail = (await stored(page))?.mailServer;
  expect(mail?.caldav).toBeNull();
  expect(mail?.accounts).toBe(0);
  const read = (await (await lea.request.get('/api/account/mail')).json()) as AccountMail;
  expect(read.mail?.linked).toBeNull();
  await lea.goto('/account');
  await expect(lea.getByRole('region', { name: NAME }).getByText('Non relié')).toBeVisible();
});

test('a disabled mail is said, and only the account kept may be removed', async ({
  page,
  newContext,
}) => {
  await signIn(page, demo.paul);
  await declareOnTestServers(page, NAME);
  const lea = await (await newContext()).newPage();
  const asked: string[] = [];
  lea.on('dialog', (dialog) => {
    asked.push(dialog.message());
    void dialog.accept();
  });
  await signIn(lea, demo.lea);
  await linkLea(lea);
  const id = (await stored(page))?.id;
  const disabled = await page.request.patch(`/api/admin/connectors/${id}`, {
    headers: { 'X-Mimorya-Request': '1' },
    data: { enabled: false } satisfies ConnectorUpdate,
  });
  expect(disabled.status(), await disabled.text()).toBe(200);

  await lea.goto('/account');
  const section = lea.getByRole('region', { name: NAME });
  await expect(section.getByText('Relié', { exact: true })).toBeVisible();
  await expect(
    section.getByText(
      'La messagerie est désactivée, ou vous n’avez pas la permission d’utiliser les connecteurs : demandez à votre administrateur. Vous pouvez retirer votre compte.',
    ),
  ).toBeVisible();
  // Nothing to link with: only the removal.
  await expect(section.getByRole('form', { name: 'Compte de messagerie' })).toHaveCount(0);
  await expect(section.getByRole('button', { name: 'Relier mon compte' })).toHaveCount(0);
  await noError(lea);
  await section.getByRole('button', { name: 'Retirer mon compte' }).click();
  // Nothing kept, nothing to show: the section goes.
  await expect(section).toHaveCount(0);
  expect(asked).toEqual([
    `Retirer votre compte de « ${NAME} » ? Son mot de passe chiffré est effacé : l’agent ne lit plus votre boîte ni votre agenda.`,
  ]);
  const read = (await (await lea.request.get('/api/account/mail')).json()) as AccountMail;
  expect(read.mail).toBeNull();
  expect((await stored(page))?.mailServer?.accounts).toBe(0);
});
