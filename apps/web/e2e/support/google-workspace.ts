// The company's Google Workspace client in the end-to-end tests (ADR-121,
// ADR-124): declared from « Connecteurs », read back from the API with the
// link a person keeps there, linked at the test provider (which plays
// Google), and deleted after each test with its links.
import type { Locator, Page } from '@playwright/test';
import type { AccountLinks, ConnectorView, Connectors } from '../../lib/api/contract';
import { cardWith, openAdmin } from './admin';
import { expect } from './fixtures';

/** The client the test provider accepts as Google's, and its secret. */
export const CLIENT = '123456789012-mimorya.apps.googleusercontent.com';
export const SECRET = 'secret-client-de-test-0123456789';
/** The label of the option that lets its tools write (ADR-123). */
export const WRITING = 'Permettre l’envoi d’e-mails et la création de rendez-vous, chacun validé';
const WRITE = { 'X-Mimorya-Request': '1' };
const TYPE = 'google_workspace';

/** Deletes the clients a test declared, with their links. */
export async function deleteClients(page: Page) {
  const listed = await page.request.get('/api/admin/connectors');
  // 401: the test never signed in; 403: not as an administrator. Either
  // way, it declared nothing.
  if (listed.status() === 401 || listed.status() === 403) return;
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.type === TYPE)) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
}

/** The company's client as the API records it, and the whole listing. */
export async function stored(page: Page): Promise<{ client?: ConnectorView; listing: string }> {
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { instances } = JSON.parse(listing) as Connectors;
  return { client: instances.find((c) => c.type === TYPE), listing };
}

/** The person's link at the client, as the API lists it to them. */
export async function linkOf(page: Page) {
  const listed = await page.request.get('/api/account/links');
  expect(listed.status()).toBe(200);
  const { connectors } = (await listed.json()) as AccountLinks;
  return connectors.find((c) => c.type === TYPE);
}

/** Declares the client from « Connecteurs », its tools writing or not;
 * returns its card. */
export async function declare(page: Page, name: string, writing: boolean): Promise<Locator> {
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Déclarer le client Google Workspace' }).click();
  const form = page.getByRole('form', { name: 'Client Google Workspace' });
  await form.getByLabel('Nom', { exact: true }).fill(name);
  await form.getByLabel('ID client OAuth').fill(CLIENT);
  await form.getByLabel('Code secret du client').fill(SECRET);
  await form.getByRole('checkbox', { name: WRITING }).setChecked(writing);
  await form.getByRole('button', { name: 'Déclarer le client' }).click();
  const card = cardWith(page, name, 'Supprimer');
  await expect(card.getByText('/api/account/links/callback')).toBeVisible();
  return card;
}
