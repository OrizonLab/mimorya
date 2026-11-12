// The second factor in the browser (ADR-200, ADR-201, ADR-203). An account
// made for the test enrols from « Mon compte » with its password: the QR
// code the server draws, its key read as an app would, the first code, the
// recovery codes shown once. Its next sign-in asks for a code after the
// password: the app's serves once, a replay is refused, a recovery code
// passes instead; it then turns its second factor off. Required by the
// company, a new account accepting its invitation enrols before any
// session and can no longer turn it off. The company's requirement goes
// back to no one, and each account is erased, whatever happened.
import type { BrowserContext, Locator, Page } from '@playwright/test';
import type { SecondFactorPolicyUpdate } from '../lib/api/contract';
import { activate, makeAccount, retire, type MadeAccounts } from './support/accounts';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { lastLink } from './support/mail';
import { App } from './support/totp';

const CHOSEN = 'phrase-de-passe-second-facteur-e2e';
const WRITE = { 'X-Mimorya-Request': '1' };
const POLICY = '/api/admin/second-factor';

const made: MadeAccounts & { paul?: Page } = { roles: [] };

test.afterEach(async () => {
  const paul = made.paul;
  made.paul = undefined;
  if (!paul) return;
  try {
    const back = await paul.request.put(POLICY, {
      headers: WRITE,
      data: { requirement: 'optional' } satisfies SecondFactorPolicyUpdate,
    });
    expect.soft(back.status()).toBe(200);
  } finally {
    await retire(paul, made);
  }
});

/** Paul, signed in in a context of his own, invites an account for the test: its address. */
async function invite(newContext: () => Promise<BrowserContext>, login: string): Promise<string> {
  const paul = await (await newContext()).newPage();
  await signIn(paul, demo.paul);
  made.paul = paul;
  const email = `${login}.${Date.now()}@neo-maintenance.example`;
  await makeAccount(paul, made, { email, displayName: 'Rose Leroy' });
  return email;
}

/** Signs out, then types the password again: the page that asks for the second factor. */
async function passwordStep(page: Page, email: string) {
  expect((await page.request.post('/api/auth/logout', { headers: WRITE })).status()).toBe(200);
  await page.goto('/login');
  await page.getByLabel('Adresse e-mail professionnelle').fill(email);
  await page.getByLabel('Mot de passe').fill(CHOSEN);
  await page.getByRole('button', { name: 'Se connecter', exact: true }).click();
  await page.waitForURL('**/login/second-factor');
}

/**
 * The enrolment shown in `scope`: its QR code drawn, its key read by an app,
 * the app's first code typed; the app and the recovery codes shown.
 */
async function enrol(scope: Page | Locator): Promise<{ app: App; codes: string[] }> {
  await expect(scope.getByRole('img', { name: /QR code/ })).toBeVisible();
  const app = new App(await scope.getByLabel('Clé du second facteur').innerText());
  await scope.getByLabel('Code à six chiffres').fill(app.next());
  await scope.getByRole('button', { name: 'Activer', exact: true }).click();
  const shown = scope.getByRole('list', { name: 'Codes de secours' }).getByRole('listitem');
  await expect(shown).toHaveCount(10);
  return { app, codes: await shown.allInnerTexts() };
}

test('an account enrols from « Mon compte » and signs in with its app or a recovery code', async ({
  page,
  newContext,
}) => {
  const email = await invite(newContext, 'second.facteur');
  await activate(page, email, CHOSEN);
  await page.goto('/account');
  const section = page.getByRole('region', { name: 'Second facteur' });
  await section.getByLabel('Mot de passe actuel').fill(CHOSEN);
  await section.getByRole('button', { name: 'Activer le second facteur' }).click();
  const { app, codes } = await enrol(section);
  const [first, second] = codes;
  if (!first || !second) throw new Error('recovery codes not shown');
  await section.getByRole('button', { name: 'J’ai noté mes codes de secours' }).click();
  await expect(section.getByRole('status')).toContainText('10 codes de secours restants');

  // The password alone opens no session: the app's code does, once.
  await passwordStep(page, email);
  const code = app.next();
  await page.getByLabel('Code à six chiffres').fill(code);
  await page.getByRole('button', { name: 'Se connecter', exact: true }).click();
  await page.waitForURL('**/chat');
  await passwordStep(page, email);
  await page.getByLabel('Code à six chiffres').fill(code);
  await page.getByRole('button', { name: 'Se connecter', exact: true }).click();
  await expect(page.getByText('Code incorrect ou déjà utilisé')).toBeVisible();
  await expect(page).toHaveURL(/\/login\/second-factor$/);

  // Without the app, a recovery code.
  await page.getByRole('button', { name: 'Utiliser un code de secours' }).click();
  await page.getByLabel('Code de secours').fill(first);
  await page.getByRole('button', { name: 'Se connecter', exact: true }).click();
  await page.waitForURL('**/chat');

  // Turned off with another, as read again from the API.
  await page.goto('/account');
  await expect(section.getByRole('status')).toContainText('9 codes de secours restants');
  const off = section.getByRole('form', { name: 'Désactiver le second facteur' });
  await off.getByLabel('Code de l’application ou code de secours').fill(second);
  await off.getByRole('button', { name: 'Désactiver' }).click();
  await expect(section.getByRole('button', { name: 'Activer le second facteur' })).toBeVisible();
  await page.reload();
  await expect(section.getByRole('button', { name: 'Activer le second facteur' })).toBeVisible();
  await noError(page);
});

test('required of everyone, an invited account enrols before its first session', async ({
  page,
  newContext,
}) => {
  const email = await invite(newContext, 'enrolement.connexion');
  const required = await made.paul!.request.put(POLICY, {
    headers: WRITE,
    data: { requirement: 'everyone' } satisfies SecondFactorPolicyUpdate,
  });
  expect(required.status()).toBe(200);

  const { link } = await lastLink(email, 'invitation');
  await page.goto(link);
  await page.getByLabel('Mot de passe (12 caractères minimum)').fill(CHOSEN);
  await page.getByLabel('Confirmation').fill(CHOSEN);
  await page.getByRole('button', { name: 'Activer mon compte' }).click();
  await page.waitForURL('**/login/second-factor');
  await expect(page.getByText('Votre entreprise exige un second facteur')).toBeVisible();
  // No session yet.
  expect((await page.request.get('/api/auth/me')).status()).toBe(401);

  await page.getByRole('button', { name: 'Activer le second facteur' }).click();
  await enrol(page);
  await page.getByRole('button', { name: 'J’ai noté mes codes, continuer' }).click();
  await page.waitForURL('**/chat');

  // Required, it stays.
  await page.goto('/account');
  const section = page.getByRole('region', { name: 'Second facteur' });
  await expect(section.getByRole('status')).toContainText('exigé par votre entreprise');
  await expect(section.getByRole('form', { name: 'Désactiver le second facteur' })).toHaveCount(0);
  await noError(page);
});
