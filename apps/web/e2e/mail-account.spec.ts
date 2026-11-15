// A person's account at the company's standard mail (ADR-205), linked from
// « Mon compte » against the end-to-end API's simulated IMAP, SMTP and
// CalDAV servers: Paul declares the mail; Léa is shown where her password
// goes, a wrong password is refused and nothing is linked, the right one
// links her account (read back from the API, the password never in it);
// Paul's card counts it; she removes it, confirming first. Then the
// servers it was checked against: Paul, his tab loaded before she linked
// it, moves the IMAP server to another host and is asked first, of the
// account as the server counts it then; he declines, and nothing is sent;
// SMTP secured otherwise moves no host, asks nothing and sends only what
// changed;
// he accepts, and her account is let go. Whatever happens, the mail is
// deleted, with her account.
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
import type { AccountMail, MailAccountLink } from '../lib/api/contract';

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
  const patches: unknown[] = [];
  page.on('request', (request) => {
    if (request.method() === 'PATCH') patches.push(request.postDataJSON());
  });
  const question =
    'Les serveurs où partent les mots de passe changent : les 1 compte(s) relié(s) sont retirés, et chacun relie le sien de nouveau dans « Mon compte ». Continuer ?';

  // Another host, declined: asked of the account linked since, nothing sent.
  await card.getByRole('button', { name: 'Modifier' }).click();
  const form = card.getByRole('form', { name: 'Messagerie standard' });
  const imapHost = form.getByLabel('Serveur IMAP (TLS dès la connexion)');
  await imapHost.fill('imap.neo-maintenance.example');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect.poll(() => asked).toEqual([question]);
  expect(patches).toEqual([]);
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
  expect(patches).toEqual([{ name: NAME, smtp: secured }]);
  mail = (await stored(page))?.mailServer;
  expect(mail?.imap).toEqual(TEST_SERVERS.imap);
  expect(mail?.smtp).toEqual(secured);
  expect(mail?.caldav).toBe(TEST_SERVERS.caldav);
  expect(mail?.accounts).toBe(1);

  // Another host, accepted: her account is let go.
  accept = true;
  await card.getByRole('button', { name: 'Modifier' }).click();
  await imapHost.fill('imap.neo-maintenance.example');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(card.getByText(/· 0 compte\(s\) relié\(s\)$/)).toBeVisible();
  expect(asked).toEqual([question, question]);
  await noError(page);
  mail = (await stored(page))?.mailServer;
  expect(mail?.imap.host).toBe('imap.neo-maintenance.example');
  expect(mail?.accounts).toBe(0);
  const read = (await (await lea.request.get('/api/account/mail')).json()) as AccountMail;
  expect(read.mail?.linked).toBeNull();
  await lea.goto('/account');
  await expect(lea.getByRole('region', { name: NAME }).getByText('Non relié')).toBeVisible();
});
