// What the company connects its assistant to, from the administration: an
// MCP server with its token (ADR-025), tried before it is added (ADR-050),
// and its own model provider with its API key (ADR-024). Both secrets are
// write-only: never shown once saved.
// Whatever happens, the company keeps the platform's model and loses the test
// server: the scenario that follows runs with them.
import type { ConnectorTest, Connectors } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, FOREIGN_HOST, noError, test } from './support/fixtures';

const SERVER = 'Documents e2e';
const WRITE = { 'X-Mimorya-Request': '1' };

test.beforeEach(({ page }) => {
  // Deletions and the return to the platform's model ask for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  // With the test's session. 404: the platform's model is already in use;
  // 401: the test never signed in, so changed nothing.
  const reset = await page.request.delete('/api/admin/llm', { headers: WRITE });
  expect([200, 401, 404]).toContain(reset.status());
  if (reset.status() === 401) return;
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.name === SERVER)) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

test('an MCP server is added with its token, disabled and deleted', async ({ page }) => {
  const token = 'jeton-e2e-secret';
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Ajouter un serveur MCP' }).click();
  await page.getByLabel('Nom').fill(SERVER);
  await page.getByLabel('Préfixe des outils (définitif)').fill('doce');
  // Never reached: saving does not connect to the server.
  await page.getByLabel(/^Adresse du serveur/).fill(`https://mcp.${FOREIGN_HOST}/mcp`);
  await page.getByLabel(/^Jeton d’accès/).fill(token);
  await page.getByRole('button', { name: 'Ajouter le serveur' }).click();

  const card = (button: string) => cardWith(page, SERVER, button);
  await expect(card('Désactiver').getByText('jeton enregistré')).toBeVisible();
  expect(await page.content()).not.toContain(token);

  await card('Désactiver').getByRole('button', { name: 'Désactiver', exact: true }).click();
  // The status the API now gives it, before anything else is sent.
  await expect(card('Activer').getByText('Désactivé', { exact: true })).toBeVisible();
  await noError(page);

  await card('Supprimer').getByRole('button', { name: 'Supprimer', exact: true }).click();
  await expect(page.getByText(SERVER, { exact: true })).toHaveCount(0);
  await noError(page);
});

test('an MCP server is tried before it is added, and nothing is saved', async ({ page }) => {
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Ajouter un serveur MCP' }).click();
  await page.getByLabel('Nom').fill(SERVER);
  await page.getByLabel('Préfixe des outils (définitif)').fill('doce');
  // A name of the private network: the platform refuses to connect to it.
  const address = page.getByLabel(/^Adresse du serveur/);
  await address.fill('https://localhost/mcp');
  const trial = page.getByRole('button', { name: 'Tester avant d’enregistrer' });
  await trial.click();
  const refused = page.getByText(/^Échec : Adresse refusée/);
  await expect(refused).toBeVisible();
  // The trial answered for that address: another one sets it aside.
  await address.fill('https://10.0.0.7/mcp');
  await expect(refused).toHaveCount(0);
  // An address of the private network, refused before any connection: the
  // refusal too is set aside by another address.
  await trial.click();
  const privateAddress = page.getByText('Adresse d’un réseau privé : refusée sur ce serveur');
  await expect(privateAddress).toBeVisible();
  await address.fill(`https://mcp.${FOREIGN_HOST}/mcp`);
  await expect(privateAddress).toHaveCount(0);

  // A server that answers (the API's answer stands in for it here).
  await page.route('**/api/admin/connectors/trial', (route) =>
    route.fulfill({
      json: {
        ok: true,
        error: null,
        tools: [
          {
            name: 'mcp_doce_search',
            title: 'Rechercher',
            description: 'Recherche dans les documents',
            enabled: true,
            risk: 'write',
            permissions: ['connector:use'],
          },
        ],
      } satisfies ConnectorTest,
    }),
  );
  await trial.click();
  await expect(
    page.getByText(/Connexion réussie : 1 outil\(s\) exposé\(s\) \(mcp_doce_search\)/),
  ).toBeVisible();

  // Tried, never added.
  await page.getByRole('button', { name: 'Annuler' }).click();
  const listed = await page.request.get('/api/admin/connectors');
  const { instances } = (await listed.json()) as Connectors;
  expect(instances.filter((c) => c.name === SERVER)).toEqual([]);
  await noError(page);
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
  expect(await page.content()).not.toContain(key);

  await page.getByRole('button', { name: 'Revenir au modèle de la plateforme' }).click();
  await expect(page.getByText(/Fournisseur de votre entreprise/)).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'Utiliser le fournisseur de votre entreprise' }),
  ).toBeVisible();
  await noError(page);
});
