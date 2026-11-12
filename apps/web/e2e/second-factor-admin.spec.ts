// The second factor from the administration (ADR-201, ADR-202). Who manages
// the company's settings chooses, in « Connexion », who must have one: the
// choice saved is read again after a reload. Who manages the accounts sees,
// in « Utilisateurs », whose second factor is on and resets an account's:
// its session ends, the mark goes, and its password alone signs it in
// again. The requirement goes back to no one, and the account made is
// erased, whatever happened.
import type { Page } from '@playwright/test';
import type {
  SecondFactorCodeInput,
  SecondFactorEnrolment,
  SecondFactorEnrolmentRequest,
  SecondFactorPolicyUpdate,
  SecondFactorPolicyView,
} from '../lib/api/contract';
import { activate, makeAccount, retire, type MadeAccounts } from './support/accounts';
import { openAdmin } from './support/admin';
import { expect, noError, signIn, test } from './support/fixtures';
import { App } from './support/totp';

const CHOSEN = 'phrase-de-passe-reinitialisation-e2e';
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

test('the company chooses who must have a second factor', async ({ page }) => {
  await openAdmin(page, 'Connexion');
  made.paul = page;
  const section = page.getByRole('region', { name: 'Second facteur' });
  const administrators = section.getByRole('radio', { name: /^Les administrateurs/ });
  await expect(section.getByRole('radio', { name: /^Personne/ })).toBeChecked();
  await administrators.check();
  await section.getByRole('button', { name: 'Enregistrer l’exigence' }).click();
  await expect(section.getByRole('status')).toContainText('Enregistré');

  // As saved: read again after a reload, and by the API.
  await page.reload();
  await page.getByRole('tab', { name: 'Connexion' }).click();
  await expect(administrators).toBeChecked();
  await expect(section.getByRole('button', { name: 'Enregistrer l’exigence' })).toBeDisabled();
  const policy = (await (await page.request.get(POLICY)).json()) as SecondFactorPolicyView;
  expect(policy.requirement).toBe('administrators');
  await noError(page);
});

test('an administrator resets an account’s second factor', async ({ page, newContext }) => {
  await openAdmin(page, 'Utilisateurs');
  made.paul = page;
  const email = `reinitialisation.${Date.now()}@neo-maintenance.example`;
  await makeAccount(page, made, { email, displayName: 'Rose Leroy' });

  // Rose turns hers on, as her app does.
  const rose = await (await newContext()).newPage();
  await activate(rose, email, CHOSEN);
  const started = await rose.request.post('/api/account/second-factor/enrolment', {
    headers: WRITE,
    data: { password: CHOSEN } satisfies SecondFactorEnrolmentRequest,
  });
  expect(started.status()).toBe(200);
  const app = new App(((await started.json()) as SecondFactorEnrolment).secret);
  const confirmed = await rose.request.post('/api/account/second-factor/confirmation', {
    headers: WRITE,
    data: { code: app.next() } satisfies SecondFactorCodeInput,
  });
  expect(confirmed.status()).toBe(200);

  // Paul sees it on, and resets it.
  await page.reload();
  await page.getByRole('tab', { name: 'Utilisateurs' }).click();
  const row = page.getByRole('row').filter({ hasText: email }).first();
  await expect(row.getByText('Second facteur', { exact: true })).toBeVisible();
  page.on('dialog', (dialog) => dialog.accept());
  await row.getByRole('button', { name: 'Gérer' }).click();
  await page.getByRole('button', { name: 'Réinitialiser le second facteur' }).click();
  await expect(page.getByText('Second facteur de Rose Leroy réinitialisé')).toBeVisible();
  await page.reload();
  await page.getByRole('tab', { name: 'Utilisateurs' }).click();
  await expect(page.getByRole('row').filter({ hasText: email }).first()).toBeVisible();
  await expect(row.getByText('Second facteur', { exact: true })).toHaveCount(0);

  // Rose's session ended; her password alone signs her in again.
  expect((await rose.request.get('/api/auth/me')).status()).toBe(401);
  await signIn(rose, email, CHOSEN);
  await noError(page);
});
