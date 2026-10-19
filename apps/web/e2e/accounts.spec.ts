// Getting into an account through an e-mailed link (ADR-027, ADR-028,
// ADR-029): a newcomer's invitation, a forgotten password, a reset link sent
// by an administrator who never sees it.
import { demo, DEMO_PASSWORD, expect, signIn, test, watch } from './support/fixtures';
import { lastLink, mailCount } from './support/mail';

const CHOSEN = 'phrase-de-passe-choisie-e2e';

test('a newcomer activates the account from the e-mailed invitation', async ({
  page,
  browser,
  violations,
}) => {
  const email = 'zoe.leroy@neo-maintenance.example';
  await signIn(page, demo.paul);
  await page.goto('/admin');
  await page.getByRole('tab', { name: 'Utilisateurs' }).click();
  await page.getByRole('button', { name: 'Ajouter un collaborateur' }).click();
  await page.getByLabel('Nom affiché').fill('Zoé Leroy');
  await page.getByLabel('Adresse e-mail').fill(email);
  await page.getByRole('button', { name: 'Créer le compte' }).click();
  await expect(page.getByText(`Invitation envoyée à ${email}`)).toBeVisible();

  const { link, mail } = await lastLink(email, 'invitation');
  expect(mail.text).toContain('Paul Renaud vous a créé un compte');
  const context = await browser.newContext();
  await watch(context, violations);
  const newcomer = await context.newPage();
  await newcomer.goto(link);
  await expect(newcomer.getByText('Bienvenue, Zoé Leroy')).toBeVisible();
  await newcomer.getByLabel('Mot de passe (12 caractères minimum)').fill(CHOSEN);
  await newcomer.getByLabel('Confirmation').fill(CHOSEN);
  await newcomer.getByRole('button', { name: 'Activer mon compte' }).click();
  await newcomer.waitForURL('**/chat');
  await context.close();
});

test('a forgotten password is replaced through the e-mailed link, once', async ({
  page,
  browser,
  violations,
}) => {
  await page.goto('/login');
  await page.getByRole('link', { name: 'Mot de passe oublié ?' }).click();
  await page.getByLabel('Adresse e-mail professionnelle').fill(demo.hugo);
  await page.getByRole('button', { name: 'Recevoir un lien' }).click();
  await expect(page.getByText('Demande enregistrée')).toBeVisible();

  const { link } = await lastLink(demo.hugo, 'reset-password');
  await page.goto(link);
  await expect(page.getByText(`Choisissez le nouveau mot de passe de ${demo.hugo}`)).toBeVisible();
  await page.getByLabel('Mot de passe (12 caractères minimum)').fill(CHOSEN);
  await page.getByLabel('Confirmation').fill(CHOSEN);
  await page.getByRole('button', { name: 'Enregistrer et me connecter' }).click();
  await page.waitForURL('**/chat');

  const context = await browser.newContext();
  await watch(context, violations);
  const later = await context.newPage();
  await later.goto(link);
  await expect(later.getByText('Ce lien de réinitialisation n’est plus valable')).toBeVisible();
  await signIn(later, demo.hugo, CHOSEN);
  await context.close();
});

test('an administrator e-mails a reset link without seeing it', async ({ page }) => {
  const before = await mailCount(demo.lucas);
  page.on('dialog', (dialog) => dialog.accept());
  await signIn(page, demo.paul);
  await page.goto('/admin');
  await page.getByRole('tab', { name: 'Utilisateurs' }).click();
  const row = page.getByRole('row').filter({ hasText: demo.lucas });
  await row.getByRole('button', { name: 'Gérer' }).click();
  await page.getByRole('button', { name: 'Envoyer un lien de réinitialisation' }).click();
  await expect(page.getByText('Lien de réinitialisation de Lucas Petit')).toBeVisible();
  await expect(page.getByText('/reset-password#')).toHaveCount(0);

  const { link, mail } = await lastLink(demo.lucas, 'reset-password', before);
  expect(mail.text).toContain('Paul Renaud vous envoie ce lien');
  expect(link).toContain('/reset-password#');
  // The current password works until the link is used.
  const context = await page.context().browser()!.newContext();
  const lucas = await context.newPage();
  await signIn(lucas, demo.lucas, DEMO_PASSWORD);
  await context.close();
});
