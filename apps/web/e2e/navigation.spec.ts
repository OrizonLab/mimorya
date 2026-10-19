// Every screen an administrator reaches loads without an error message, a
// page error nor a policy violation (the fixture checks the last two),
// including each administration tab.
import type { Page } from '@playwright/test';
import { noError } from './support/admin';
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

/** Waits for what the screen loads, then checks that nothing failed. */
async function loadedWithoutError(page: Page) {
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  await noError(page);
}

test('every screen of an administrator loads', async ({ page }) => {
  await signIn(page, demo.paul);
  await expect(page.getByLabel('Votre message')).toBeVisible();
  for (const [path, title] of PAGES) {
    await page.goto(path);
    await expect(page.getByRole('heading', { name: title }).first()).toBeVisible();
    await loadedWithoutError(page);
  }
  for (const tab of ADMIN_TABS) {
    await page.getByRole('tab', { name: tab }).click();
    await expect(page.getByRole('tab', { name: tab })).toHaveAttribute('aria-selected', 'true');
    await loadedWithoutError(page);
  }
});

test('the public pages open without a session', async ({ page }) => {
  for (const path of ['/login', '/forgot-password', '/invitation', '/reset-password']) {
    await page.goto(path);
    await expect(page.getByText('Votre collaborateur IA d’entreprise')).toBeVisible();
  }
});
