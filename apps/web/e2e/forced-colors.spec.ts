// The marks the interface draws over its checkboxes and radio buttons
// (ADR-045) stay visible in forced colors (Windows high contrast): drawn in
// the text's colour, not painted as the background like other backgrounds.
// A server at a foreign host, its connection test answered here, gives a
// checked checkbox (its tool's rule) and a checked radio (its OAuth form's
// client choice). What the test adds is deleted, even when it fails.
import type { ConnectorTest, Connectors, TestedTool } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, FOREIGN_HOST, noError, test } from './support/fixtures';

const SERVER = 'Contraste e2e';
const WRITE = { 'X-Mimorya-Request': '1' };

test.afterEach(async ({ page }) => {
  const response = await page.request.get('/api/admin/connectors');
  if (response.status() === 401) return;
  const { instances } = (await response.json()) as Connectors;
  for (const connector of instances.filter((c) => c.name === SERVER)) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

test('the marks drawn over checkboxes and radios stay visible in forced colors', async ({
  page,
}) => {
  const tool: TestedTool = {
    name: 'mcp_cont_search',
    title: 'mcp_cont_search',
    description: 'Outil e2e',
    replacedDescription: null,
    enabled: true,
    risk: 'read',
    permissions: ['connector:use'],
  };
  await page.emulateMedia({ forcedColors: 'active' });
  await page.route('**/api/admin/connectors/*/test', (route) =>
    route.fulfill({ json: { ok: true, error: null, tools: [tool] } satisfies ConnectorTest }),
  );
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Ajouter un serveur MCP' }).click();
  await page.getByLabel('Nom').fill(SERVER);
  await page.getByLabel('Préfixe des outils (définitif)').fill('cont');
  await page.getByLabel(/^Adresse du serveur/).fill(`https://mcp.${FOREIGN_HOST}/mcp`);
  await page.getByRole('button', { name: 'Ajouter le serveur' }).click();
  const card = cardWith(page, SERVER, 'Tester la connexion');
  // The text's colour and the background's, as forced colors give them.
  const text = await page.evaluate(() => getComputedStyle(document.body).color);
  const background = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(text).not.toBe(background);

  // A checked checkbox: its check in the text's colour.
  await card.getByRole('button', { name: 'Tester la connexion' }).click();
  const checkbox = card.getByRole('checkbox', { name: /mcp_cont_search/ });
  await expect(checkbox).toBeChecked();
  const check = checkbox.locator('xpath=following-sibling::*[1]');
  await expect(check).toHaveCSS('color', text);

  // A checked radio: its dot in the text's colour, not the background's.
  await card.getByRole('button', { name: 'Déclarer OAuth' }).click();
  const radio = card.getByRole('radio', { name: 'Client enregistré à la main' });
  await expect(radio).toBeChecked();
  const dot = radio.locator('xpath=following-sibling::*[1]');
  await expect(dot).toHaveCSS('background-color', text);
  await noError(page);
});
