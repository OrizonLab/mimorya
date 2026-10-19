// Every screen an administrator reaches opens without error nor policy
// violation (the fixture checks both), including each administration tab.
import { demo, expect, signIn, test } from './support/fixtures';

const PAGES = [
  ['/approvals', 'Validations'],
  ['/operations', 'Opérations'],
  ['/memory', 'Mémoire'],
  ['/account', 'Mon compte'],
  ['/admin', 'Administration'],
] as const;

const ADMIN_TABS = [
  'Organisation',
  'Utilisateurs',
  'Rôles',
  'Politiques d’accès',
  'Journal d’audit',
  'Consommation',
  'Modèle d’IA',
  'Connecteurs',
  'Compétences',
];

test('every screen of an administrator opens', async ({ page }) => {
  await signIn(page, demo.paul);
  await expect(page.getByLabel('Votre message')).toBeVisible();
  for (const [path, title] of PAGES) {
    await page.goto(path);
    await expect(page.getByRole('heading', { name: title }).first()).toBeVisible();
  }
  for (const tab of ADMIN_TABS) {
    await page.getByRole('tab', { name: tab }).click();
    await expect(page.getByRole('tab', { name: tab })).toHaveAttribute('aria-selected', 'true');
  }
});

test('the public pages open without a session', async ({ page }) => {
  for (const path of ['/login', '/forgot-password', '/invitation', '/reset-password']) {
    await page.goto(path);
    await expect(page.getByText('Votre collaborateur IA d’entreprise')).toBeVisible();
  }
});
