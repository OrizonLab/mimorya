// The administration's forms, submitted to the API (whose request bodies are
// strict, ADR-040), and what they changed read back from it: a custom role, a
// deny policy, an organisation unit and an account's profile, attributes and
// rights. What a test creates it deletes, except the accounts (the API deletes
// none), whose addresses are their own and which keep no business role.
import type { Directory } from '../lib/api/contract';
import { cardWith, openAdmin, selectContaining } from './support/admin';
import { expect, noError, test } from './support/fixtures';

test.beforeEach(({ page }) => {
  // Deletions and revocations ask for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test('a custom role is created, changed and deleted', async ({ page }) => {
  await openAdmin(page, 'Rôles');
  await page.getByRole('button', { name: 'Nouveau rôle' }).click();
  await page.getByLabel('Nom').fill('Planificateur e2e');
  await page.getByLabel('Description').fill('Prépare les plannings');
  await page.getByRole('checkbox', { name: 'Consulter les plannings' }).check();
  await page.getByRole('button', { name: 'Créer le rôle' }).click();
  await expect(page.getByText('Prépare les plannings')).toBeVisible();

  await page.getByRole('button', { name: 'Modifier Planificateur e2e' }).click();
  await page.getByLabel('Description').fill('Prépare et propose les plannings');
  await page.getByRole('checkbox', { name: 'Proposer des plannings' }).check();
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  const role = cardWith(page, 'Prépare et propose les plannings', 'Modifier Planificateur e2e');
  await expect(role.getByText('Proposer des plannings', { exact: true })).toBeVisible();
  await noError(page);

  await page.getByRole('button', { name: 'Supprimer Planificateur e2e' }).click();
  await expect(page.getByText('Planificateur e2e', { exact: true })).toHaveCount(0);
});

test('a deny policy is created, disabled and deleted', async ({ page }) => {
  const description = 'Intérimaires : pas de consultation (e2e)';
  await openAdmin(page, 'Politiques d’accès');
  await page.getByRole('button', { name: 'Nouvelle politique' }).click();
  await page.getByLabel('Description (ce que la politique interdit, et à qui)').fill(description);
  await page.getByRole('checkbox', { name: /^Consulter les interventions/ }).check();
  await page.getByLabel('Attribut').fill('subject.attributes.contractType');
  await page.getByLabel('Valeur').fill('interim_e2e');
  await page.getByRole('button', { name: 'Créer la politique' }).click();
  // The card, as the API lists the policy: what it restricts, and when.
  const policy = page.locator('div').filter({ hasText: description }).filter({ hasText: 'Quand' });
  await expect(policy.last().getByText('Consulter les interventions')).toBeVisible();
  await expect(policy.last().getByText(/interim_e2e/)).toBeVisible();

  await cardWith(page, description, 'Désactiver')
    .getByRole('button', { name: 'Désactiver', exact: true })
    .click();
  await expect(cardWith(page, description, 'Activer').getByText('Désactivée')).toBeVisible();
  await noError(page);

  await page.getByRole('button', { name: `Supprimer ${description}` }).click();
  await expect(page.getByText(description, { exact: true })).toHaveCount(0);
});

test('an organisation unit is added, renamed and deleted', async ({ page }) => {
  await openAdmin(page, 'Organisation');
  await page.getByRole('button', { name: 'Ajouter une sous-unité à Équipe Nord' }).click();
  await page.getByLabel('Nom').fill('Cellule e2e');
  await page.getByRole('button', { name: 'Ajouter', exact: true }).click();
  await expect(page.getByText('Cellule e2e', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Modifier Cellule e2e' }).click();
  await page.getByLabel('Nom').fill('Cellule e2e renommée');
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(page.getByText('Cellule e2e renommée', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Supprimer Cellule e2e renommée' }).click();
  await expect(page.getByText('Cellule e2e renommée', { exact: true })).toHaveCount(0);
});

test('an account’s profile, attributes and rights are changed', async ({ page }) => {
  const email = 'ines.marchand@neo-maintenance.example';
  await openAdmin(page, 'Utilisateurs');
  await page.getByRole('button', { name: 'Ajouter un collaborateur' }).click();
  await page.getByLabel('Nom affiché').fill('Inès Marchand');
  await page.getByLabel('Adresse e-mail').fill(email);
  await page.getByRole('button', { name: 'Créer le compte' }).click();
  await expect(page.getByText(`Invitation envoyée à ${email}`)).toBeVisible();

  const row = page.getByRole('row').filter({ hasText: email }).first();
  await row.getByRole('button', { name: 'Gérer' }).click();
  await page.getByLabel('Fonction').fill('Technicienne itinérante');
  await page.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(row.getByText('Technicienne itinérante')).toBeVisible();

  await page.getByRole('button', { name: 'Ajouter un attribut' }).click();
  await page.getByLabel('Nom', { exact: true }).fill('department');
  await page.getByLabel('Valeur').fill('technique');
  await page.getByRole('button', { name: 'Enregistrer les attributs' }).click();
  // What the API saved...
  await expect
    .poll(async () => {
      const response = await page.request.get('/api/admin/users');
      if (response.status() !== 200) return `HTTP ${response.status()}`;
      const directory = (await response.json()) as Directory;
      return directory.users.find((user) => user.email === email)?.attributes;
    })
    .toEqual({ department: 'technique' });
  await noError(page);
  // ...as a page loaded afresh shows it: its form reads the directory it loads.
  await page.reload();
  await page.getByRole('tab', { name: 'Utilisateurs' }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  await row.getByRole('button', { name: 'Gérer' }).click();
  await expect(page.getByLabel('Nom', { exact: true })).toHaveValue('department');
  await expect(page.getByLabel('Valeur')).toHaveValue('technique');

  await page
    .getByRole('combobox', { name: 'Rôle', exact: true })
    .selectOption({ label: 'Responsable d’équipe' });
  await selectContaining(page.getByRole('combobox', { name: 'Unité', exact: true }), 'Équipe Nord');
  await page.getByRole('button', { name: 'Attribuer le rôle' }).click();
  const grant = row.getByText(/Responsable d’équipe .*Équipe Nord/);
  await expect(grant).toBeVisible();

  await page.getByRole('button', { name: /^Retirer Responsable d’équipe/ }).click();
  await expect(grant).toHaveCount(0);
  await noError(page);
});

test('a form reopened before the directory reloads shows what was saved', async ({ page }) => {
  const email = 'paul.renard@neo-maintenance.example';
  await openAdmin(page, 'Utilisateurs');
  await page.getByRole('button', { name: 'Ajouter un collaborateur' }).click();
  await page.getByLabel('Nom affiché').fill('Paul Renard');
  await page.getByLabel('Adresse e-mail').fill(email);
  await page.getByRole('button', { name: 'Créer le compte' }).click();
  await expect(page.getByText(`Invitation envoyée à ${email}`)).toBeVisible();

  const row = page.getByRole('row').filter({ hasText: email }).first();
  await row.getByRole('button', { name: 'Gérer' }).click();
  // From now on, the directory arrives late: the forms are reopened with the
  // account as it was listed before the save.
  await page.route('**/api/admin/users', async (route) => {
    if (route.request().method() === 'GET') await new Promise((done) => setTimeout(done, 1_500));
    await route.continue();
  });
  const reloads = () =>
    page.waitForRequest((r) => r.method() === 'GET' && r.url().endsWith('/api/admin/users'));

  let reload = reloads();
  await page.getByLabel('Fonction').fill('Chef d’équipe');
  await page.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await reload;
  await row.getByRole('button', { name: 'Fermer' }).click();
  await row.getByRole('button', { name: 'Gérer' }).click();
  // Once the directory arrives, the form shows what the API saved.
  await expect(page.getByLabel('Fonction')).toHaveValue('Chef d’équipe');

  reload = reloads();
  await page.getByRole('button', { name: 'Ajouter un attribut' }).click();
  await page.getByLabel('Nom', { exact: true }).fill('department');
  await page.getByLabel('Valeur').fill('technique');
  await page.getByRole('button', { name: 'Enregistrer les attributs' }).click();
  await reload;
  await row.getByRole('button', { name: 'Fermer' }).click();
  await row.getByRole('button', { name: 'Gérer' }).click();
  await expect(page.getByLabel('Nom', { exact: true })).toHaveValue('department');
  await expect(page.getByLabel('Valeur')).toHaveValue('technique');
  // Saved again, they keep what the first saves wrote.
  await page.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await page.getByRole('button', { name: 'Enregistrer les attributs' }).click();
  await expect
    .poll(async () => {
      const response = await page.request.get('/api/admin/users');
      if (response.status() !== 200) return `HTTP ${response.status()}`;
      const user = ((await response.json()) as Directory).users.find((u) => u.email === email);
      return [user?.jobTitle, user?.attributes];
    })
    .toEqual(['Chef d’équipe', { department: 'technique' }]);
  await noError(page);
});
