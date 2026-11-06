// The marks the interface draws over its checkboxes and radio buttons
// (ADR-045) stay visible in forced colors (Windows high contrast): drawn in
// the text's colour, not painted as the background like other backgrounds.
// A server at a foreign host, its connection test answered here, gives a
// checked checkbox (its tool's rule) and a checked radio (its OAuth form's
// client choice). What the test adds is deleted, even when it fails.
import type { ConnectorTest, TestedTool } from '../lib/api/contract';
import { addForeignServer, deleteConnectorsNamed, openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const SERVER = 'Contraste e2e';

test.afterEach(async ({ page }) => {
  // 401: the test never signed in, so added nothing.
  await deleteConnectorsNamed(page, [SERVER], [401]);
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
  const card = await addForeignServer(page, SERVER, 'cont');
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
