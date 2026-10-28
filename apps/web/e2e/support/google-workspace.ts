// The company's Google Workspace client in the end-to-end tests (ADR-121,
// ADR-124): declared from « Connecteurs », read back from the API with the
// link a person keeps there, linked at the test provider (which plays
// Google), and deleted after each test with its links, as any account
// link's (`account-links`).
import type { Locator, Page } from '@playwright/test';
import type { ConnectorView } from '../../lib/api/contract';
import { deleteConnectorsOf, linkOfType, storedOf } from './account-links';
import { cardWith, openAdmin } from './admin';
import { expect } from './fixtures';

export { linkThrough } from './account-links';

/** The client the test provider accepts as Google's, and its secret. */
export const CLIENT = '123456789012-mimorya.apps.googleusercontent.com';
export const SECRET = 'secret-client-de-test-0123456789';
/** The label of the option that lets its tools write (ADR-123). */
export const WRITING = 'Permettre l’envoi d’e-mails et la création de rendez-vous, chacun validé';
const TYPE = 'google_workspace';

/** Deletes the clients a test declared, with their links. */
export const deleteClients = (page: Page) => deleteConnectorsOf(page, TYPE);

/** The company's client as the API records it, and the whole listing. */
export async function stored(page: Page): Promise<{ client?: ConnectorView; listing: string }> {
  const { connector, listing } = await storedOf(page, TYPE);
  return { client: connector, listing };
}

/** The person's link at the client, as the API lists it to them. */
export const linkOf = (page: Page) => linkOfType(page, TYPE);

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
