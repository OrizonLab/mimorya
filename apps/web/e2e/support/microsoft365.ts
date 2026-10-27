// The company's Microsoft 365 application in the end-to-end tests (ADR-118,
// ADR-120): declared from « Connecteurs », read back from the API with the
// link a person keeps there, linked at the test provider (which plays
// Microsoft), and deleted after each test with its links.
import type { Locator, Page } from '@playwright/test';
import type { AccountLinks, ConnectorView, Connectors } from '../../lib/api/contract';
import { cardWith, openAdmin } from './admin';
import { expect } from './fixtures';

export const DIRECTORY = '72f988bf-86f1-41af-91ab-2d7cd011db47';
/** The test provider's application, and its secret. */
export const APPLICATION = '0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0';
export const SECRET = 'secret-client-de-test-0123456789';
/** The label of the option that lets its tools write (ADR-120). */
export const WRITING = 'Permettre l’envoi d’e-mails et la création de rendez-vous, chacun validé';
const WRITE = { 'X-Mimorya-Request': '1' };

/** Deletes the applications a test declared, with their links. */
export async function deleteApplications(page: Page) {
  const listed = await page.request.get('/api/admin/connectors');
  // 401: the test never signed in; 403: not as an administrator. Either
  // way, it declared nothing.
  if (listed.status() === 401 || listed.status() === 403) return;
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.type === 'microsoft365')) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
}

/** The company's application as the API records it, and the whole listing. */
export async function stored(page: Page): Promise<{ app?: ConnectorView; listing: string }> {
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { instances } = JSON.parse(listing) as Connectors;
  return { app: instances.find((c) => c.type === 'microsoft365'), listing };
}

/** The person's link at the application, as the API lists it to them. */
export async function linkOf(page: Page) {
  const listed = await page.request.get('/api/account/links');
  expect(listed.status()).toBe(200);
  const { connectors } = (await listed.json()) as AccountLinks;
  return connectors.find((c) => c.type === 'microsoft365');
}

/** Declares the application from « Connecteurs », its tools writing or not;
 * returns its card. */
export async function declare(page: Page, name: string, writing: boolean): Promise<Locator> {
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Déclarer l’application Microsoft 365' }).click();
  const form = page.getByRole('form', { name: 'Application Microsoft 365' });
  await form.getByLabel('Nom', { exact: true }).fill(name);
  await form.getByLabel('ID de l’annuaire (locataire)').fill(DIRECTORY);
  await form.getByLabel('ID de l’application (client)').fill(APPLICATION);
  await form.getByLabel('Secret de l’application').fill(SECRET);
  await form.getByRole('checkbox', { name: WRITING }).setChecked(writing);
  await form.getByRole('button', { name: 'Déclarer l’application' }).click();
  const card = cardWith(page, name, 'Supprimer');
  await expect(card.getByText('/api/account/links/callback')).toBeVisible();
  return card;
}

/** Clicks the section's link button, through the provider and back. */
export async function linkThrough(page: Page, section: Locator, button: string) {
  const back = page.waitForResponse((r) => r.url().includes('/api/account/links/callback'));
  await section.getByRole('button', { name: button }).click();
  await back;
}
