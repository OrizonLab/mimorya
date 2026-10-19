// While a screen reloads its list after a change, nothing can be changed from
// the former one: its controls are disabled until the list arrives, then the
// forms show what the API saved, and an open form what another administrator
// changed (ADR-041, ADR-042). Each test holds the list
// until it lets it through. The account it creates keeps no business role;
// the role it creates is deleted, even when it fails.
import type { Page } from '@playwright/test';
import type { AccountUpdate, Directory, Roles } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const ROLE = 'Réviseur e2e';
const WRITE = { 'X-Mimorya-Request': '1' };

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/admin/roles');
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { roles } = (await listed.json()) as Roles;
  for (const role of roles.filter((r) => r.name === ROLE)) {
    const deleted = await page.request.delete(`/api/admin/roles/${role.id}`, { headers: WRITE });
    expect(deleted.status()).toBe(200);
  }
});

/**
 * Holds the list at `path` once armed: `arm()` before the change that reloads
 * it resolves when the reload is sent, `release()` lets it through.
 */
async function holdList(page: Page, path: string) {
  let held = Promise.resolve();
  let release = () => {};
  await page.route(`**${path}`, async (route) => {
    if (route.request().method() === 'GET') await held;
    await route.continue();
  });
  return {
    arm: () => {
      held = new Promise<void>((done) => (release = done));
      return page.waitForRequest((r) => r.method() === 'GET' && r.url().endsWith(path));
    },
    release: () => release(),
  };
}

test('an account’s forms wait for, then follow, the reloaded directory', async ({ page }) => {
  const email = 'victor.chassagne@neo-maintenance.example';
  await openAdmin(page, 'Utilisateurs');
  await page.getByRole('button', { name: 'Ajouter un collaborateur' }).click();
  await page.getByLabel('Nom affiché').fill('Victor Chassagne');
  await page.getByLabel('Adresse e-mail').fill(email);
  await page.getByRole('button', { name: 'Créer le compte' }).click();
  await expect(page.getByText(`Invitation envoyée à ${email}`)).toBeVisible();

  const row = page.getByRole('row').filter({ hasText: email }).first();
  await row.getByRole('button', { name: 'Gérer' }).click();
  const directory = await holdList(page, '/api/admin/users');
  const jobTitle = page.getByLabel('Fonction');
  const saveProfile = page.getByRole('button', { name: 'Enregistrer', exact: true });
  const saveAttributes = page.getByRole('button', { name: 'Enregistrer les attributs' });

  let reload = directory.arm();
  await jobTitle.fill('Chef d’équipe');
  await saveProfile.click();
  await reload;
  // Until the directory arrives, nothing on the screen can be changed, nor
  // the account closed and reopened from the former directory.
  await expect(row.getByRole('button', { name: 'Fermer' })).toBeDisabled();
  await expect(jobTitle).toBeDisabled();
  await expect(saveProfile).toBeDisabled();
  await expect(saveAttributes).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Ajouter un collaborateur' })).toBeDisabled();
  directory.release();
  await expect(saveProfile).toBeEnabled();
  await expect(jobTitle).toHaveValue('Chef d’équipe');

  // Another administrator changes the account while its form is open...
  const listed = await page.request.get('/api/admin/users');
  const id = ((await listed.json()) as Directory).users.find((u) => u.email === email)?.id;
  const changed = await page.request.patch(`/api/admin/users/${id}`, {
    headers: WRITE,
    data: { jobTitle: 'Responsable de secteur' } satisfies AccountUpdate,
  });
  expect(changed.status()).toBe(200);
  // ...the next reload brings the change to the form, which saving would
  // otherwise overwrite with the former value.
  reload = directory.arm();
  await page.getByRole('button', { name: 'Ajouter un attribut' }).click();
  await page.getByLabel('Nom', { exact: true }).fill('department');
  await page.getByLabel('Valeur').fill('technique');
  await saveAttributes.click();
  await reload;
  await expect(jobTitle).toHaveValue('Chef d’équipe');
  directory.release();
  await expect(jobTitle).toHaveValue('Responsable de secteur');
  await expect(page.getByLabel('Valeur')).toHaveValue('technique');
  await noError(page);
});

test('a role is edited again only from the reloaded list', async ({ page }) => {
  page.on('dialog', (dialog) => dialog.accept());
  await openAdmin(page, 'Rôles');
  await page.getByRole('button', { name: 'Nouveau rôle' }).click();
  await page.getByLabel('Nom').fill(ROLE);
  await page.getByLabel('Description').fill('Relit les plannings');
  await page.getByRole('checkbox', { name: 'Consulter les plannings' }).check();
  await page.getByRole('button', { name: 'Créer le rôle' }).click();
  const edit = page.getByRole('button', { name: `Modifier ${ROLE}` });
  await expect(edit).toBeEnabled();

  const roles = await holdList(page, '/api/admin/roles');
  const reload = roles.arm();
  await edit.click();
  await page.getByLabel('Description').fill('Relit et commente les plannings');
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await reload;
  // The card is back, as the former list shows it: it cannot be edited again
  // from there, nor can anything else on the screen be changed.
  await expect(page.getByText('Relit les plannings')).toBeVisible();
  await expect(edit).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Nouveau rôle' })).toBeDisabled();
  roles.release();
  await expect(page.getByText('Relit et commente les plannings')).toBeVisible();
  await edit.click();
  await expect(page.getByLabel('Description')).toHaveValue('Relit et commente les plannings');
  await page.getByRole('button', { name: 'Annuler' }).click();
  await noError(page);

  await page.getByRole('button', { name: `Supprimer ${ROLE}` }).click();
  await expect(page.getByText(ROLE, { exact: true })).toHaveCount(0);
});
