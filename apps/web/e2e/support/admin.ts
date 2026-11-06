import type { Locator, Page } from '@playwright/test';
import type { Connectors } from '../../lib/api/contract';
import { demo, expect, FOREIGN_HOST, signIn } from './fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };

/**
 * A test's cleanup: deletes the company's connectors named in `names`, as
 * `page`'s session, each deletion expected to succeed. A listing answered
 * with one of `skipped` means the test added nothing (401: it never signed
 * in; 403: not as an administrator); any other failure is said.
 */
export async function deleteConnectorsNamed(page: Page, names: string[], skipped: number[] = []) {
  const response = await page.request.get('/api/admin/connectors');
  if (skipped.includes(response.status())) return;
  expect(response.status()).toBe(200);
  const { instances } = (await response.json()) as Connectors;
  for (const connector of instances.filter((c) => names.includes(c.name))) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
}

/** Signs in as the demo administrator and opens a tab of the administration. */
export async function openAdmin(page: Page, tab: string) {
  await signIn(page, demo.paul);
  await page.goto('/admin');
  await page.getByRole('tab', { name: tab }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
}

/** The administration's « Connecteurs » tab, the administrator signed in already. */
export async function connectorsTab(page: Page) {
  await page.goto('/admin');
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
}

/**
 * The innermost block showing `text` and a button named exactly `button`:
 * one card's row. Locators keep the document's order, where a block comes
 * before the blocks it contains, so the last match is the innermost one.
 * Anchor it on a button every card of the list has (« Supprimer »): one
 * only some cards have (« Activer ») may name a block holding several of
 * them while the card sought does not show it yet.
 */
export function cardWith(page: Page, text: string, button: string): Locator {
  return page
    .locator('div')
    .filter({ hasText: text })
    .filter({ has: page.getByRole('button', { name: button, exact: true }) })
    .last();
}

/** Adds, from the open « Connecteurs » tab, an MCP server at `address`. */
export async function addServer(page: Page, name: string, prefix: string, address: string) {
  await page.getByRole('button', { name: 'Ajouter un serveur MCP' }).click();
  await page.getByLabel('Nom').fill(name);
  await page.getByLabel('Préfixe des outils (définitif)').fill(prefix);
  await page.getByLabel(/^Adresse du serveur/).fill(address);
  await page.getByRole('button', { name: 'Ajouter le serveur' }).click();
}

/**
 * Adds, from the open « Connecteurs » tab, an MCP server at a foreign host
 * the platform never reaches (its connection test is answered by the test's
 * own route): the card that shows it.
 */
export async function addForeignServer(page: Page, name: string, prefix: string) {
  await addServer(page, name, prefix, `https://mcp.${FOREIGN_HOST}/mcp`);
  return cardWith(page, name, 'Tester la connexion');
}

/** Selects the option whose text contains `text` (unit names are indented). */
export async function selectContaining(select: Locator, text: string) {
  const value = await select.locator('option', { hasText: text }).first().getAttribute('value');
  await select.selectOption(value ?? '');
}
