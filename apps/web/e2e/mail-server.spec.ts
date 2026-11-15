// The company's standard mail declared from « Connecteurs » (ADR-205): Paul
// declares its IMAP and SMTP servers, reading only, and checks the
// declaration (two tools; the servers are not reached). He then secures
// SMTP by STARTTLS (its usual port following), adds a CalDAV agenda and
// lets its tools write: five tools. An agenda in clear is refused, the
// declaration unchanged. He deletes it. Each step is read back from the
// API. Whatever happens, the mail is deleted.
import { expect, noError, test } from './support/fixtures';
import { WRITING, declare, deleteMailServers, stored } from './support/mail-server';

const NAME = 'Messagerie e2e';
const IMAP = 'imap.neo-maintenance.example';
const SMTP = 'smtp.neo-maintenance.example';
const AGENDA = 'https://agenda.neo-maintenance.example/dav/';

test.afterEach(async ({ page }) => deleteMailServers(page));

test('the standard mail is declared, changed and deleted', async ({ page }) => {
  const asked: string[] = [];
  page.on('dialog', (dialog) => {
    asked.push(dialog.message());
    void dialog.accept();
  });
  const card = await declare(page, NAME, IMAP, SMTP);
  await expect(
    card.getByText(
      `IMAP ${IMAP}:993 · SMTP ${SMTP}:465 (TLS dès la connexion) · outils mail_… · 0 compte(s) relié(s)`,
    ),
  ).toBeVisible();
  await expect(card.getByText(/^Lecture seule : ni envoi d’e-mails/)).toBeVisible();
  // One per company: no second declaration is offered.
  await expect(page.getByRole('button', { name: 'Déclarer la messagerie standard' })).toHaveCount(
    0,
  );
  await noError(page);
  let mail = await stored(page);
  expect(mail?.name).toBe(NAME);
  expect(mail?.mailServer).toEqual({
    imap: { host: IMAP, port: 993 },
    smtp: { host: SMTP, port: 465, security: 'tls' },
    caldav: null,
    writeTools: false,
    accounts: 0,
  });

  // Its declaration checked without reaching its servers: reading tools.
  await card.getByRole('button', { name: 'Vérifier la déclaration' }).click();
  await expect(
    card.getByText(/^Déclaration vérifiée : 2 outil\(s\)\. Les serveurs de messagerie ne sont pas/),
  ).toBeVisible();

  // STARTTLS on its usual port, an agenda, writing.
  await card.getByRole('button', { name: 'Modifier' }).click();
  const form = card.getByRole('form', { name: 'Messagerie standard' });
  await form.getByLabel('Sécurité SMTP').selectOption('starttls');
  await expect(form.getByLabel('Port SMTP')).toHaveValue('587');
  await form.getByLabel('Agenda CalDAV (facultatif, adresse en https://)').fill(AGENDA);
  await form.getByRole('checkbox', { name: WRITING }).check();
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(card.getByText(`SMTP ${SMTP}:587 (STARTTLS exigé)`, { exact: false })).toBeVisible();
  await expect(card.getByText(`Agenda CalDAV ${AGENDA}`)).toBeVisible();
  await expect(card.getByText(/^Envoi d’e-mails et création de rendez-vous permis/)).toBeVisible();
  // No account was linked: nothing to confirm.
  expect(asked).toEqual([]);
  mail = await stored(page);
  expect(mail?.mailServer?.smtp).toEqual({ host: SMTP, port: 587, security: 'starttls' });
  expect(mail?.mailServer?.caldav).toBe(AGENDA);
  expect(mail?.mailServer?.writeTools).toBe(true);
  await card.getByRole('button', { name: 'Vérifier la déclaration' }).click();
  await expect(card.getByText(/^Déclaration vérifiée : 5 outil\(s\)\./)).toBeVisible();

  // An agenda in clear: refused, nothing changed.
  await card.getByRole('button', { name: 'Modifier' }).click();
  await form
    .getByLabel('Agenda CalDAV (facultatif, adresse en https://)')
    .fill('http://agenda.neo-maintenance.example/dav/');
  await form.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(form.getByText(/https:\/\/ attendue/)).toBeVisible();
  expect((await stored(page))?.mailServer?.caldav).toBe(AGENDA);
  await form.getByRole('button', { name: 'Annuler' }).click();

  await card.getByRole('button', { name: 'Supprimer' }).click();
  await expect(page.getByText(NAME)).toHaveCount(0);
  expect(asked).toEqual([
    `Supprimer la messagerie « ${NAME} » et les 0 compte(s) que les collaborateurs y ont relié(s), avec leurs mots de passe chiffrés ?`,
  ]);
  expect(await stored(page)).toBeUndefined();
  await expect(page.getByRole('button', { name: 'Déclarer la messagerie standard' })).toBeVisible();
});
