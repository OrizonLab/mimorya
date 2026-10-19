// What the company connects its assistant to, from the administration: an
// MCP server with its token (ADR-025) and its own model provider with its
// API key (ADR-024). Both secrets are write-only: never shown once saved.
// Each test leaves the demo company as it found it.
import { cardWith, openAdmin } from './support/admin';
import { expect, FOREIGN_HOST, test } from './support/fixtures';

test.beforeEach(({ page }) => {
  // Deletions and the return to the platform's model ask for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test('an MCP server is added with its token, disabled and deleted', async ({ page }) => {
  const token = 'jeton-e2e-secret';
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Ajouter un serveur MCP' }).click();
  await page.getByLabel('Nom').fill('Documents e2e');
  await page.getByLabel('Préfixe des outils (définitif)').fill('doce');
  // Never reached: saving does not connect to the server.
  await page.getByLabel(/^Adresse du serveur/).fill(`https://mcp.${FOREIGN_HOST}/mcp`);
  await page.getByLabel(/^Jeton d’accès/).fill(token);
  await page.getByRole('button', { name: 'Ajouter le serveur' }).click();

  const card = (button: string) => cardWith(page, 'Documents e2e', button);
  await expect(card('Désactiver').getByText('jeton enregistré')).toBeVisible();
  await expect(page.getByText(token)).toHaveCount(0);

  await card('Désactiver').getByRole('button', { name: 'Désactiver' }).click();
  await expect(card('Activer').getByRole('button', { name: 'Activer' })).toBeVisible();

  await card('Supprimer').getByRole('button', { name: 'Supprimer' }).click();
  await expect(page.getByText('Documents e2e', { exact: true })).toHaveCount(0);
});

test('the company chooses its own model, then goes back to the platform’s', async ({ page }) => {
  const key = 'cle-api-e2e-secrete';
  await openAdmin(page, 'Modèle d’IA');
  await page.getByRole('button', { name: 'Utiliser le fournisseur de votre entreprise' }).click();
  await page
    .getByRole('combobox', { name: 'Fournisseur' })
    .selectOption({ label: 'Anthropic (Claude)' });
  await page.getByLabel(/^Clé d’API/).fill(key);
  await page.getByRole('button', { name: 'Enregistrer' }).click();

  await expect(page.getByText(/Fournisseur de votre entreprise : Anthropic/)).toBeVisible();
  await expect(page.getByText('Clé enregistrée')).toBeVisible();
  await expect(page.getByText(key)).toHaveCount(0);

  await page.getByRole('button', { name: 'Revenir au modèle de la plateforme' }).click();
  await expect(page.getByText(/Fournisseur de votre entreprise/)).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'Utiliser le fournisseur de votre entreprise' }),
  ).toBeVisible();
});
