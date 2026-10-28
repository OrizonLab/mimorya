// The company's Microsoft 365 application in the end-to-end tests (ADR-118,
// ADR-120): declared from « Connecteurs », read back from the API with the
// link a person keeps there, linked at the test provider (which plays
// Microsoft), and deleted after each test with its links, as any account
// link's (`account-links`).
import type { Locator, Page } from '@playwright/test';
import type { ConnectorView } from '../../lib/api/contract';
import { deleteConnectorsOf, linkOfType, storedOf } from './account-links';
import { cardWith, openAdmin } from './admin';
import { expect } from './fixtures';

export { linkThrough } from './account-links';

export const DIRECTORY = '72f988bf-86f1-41af-91ab-2d7cd011db47';
/** The test provider's application, and its secret. */
export const APPLICATION = '0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0';
export const SECRET = 'secret-client-de-test-0123456789';
/** The label of the option that lets its tools write (ADR-120). */
export const WRITING = 'Permettre l’envoi d’e-mails et la création de rendez-vous, chacun validé';
const TYPE = 'microsoft365';

/** Deletes the applications a test declared, with their links. */
export const deleteApplications = (page: Page) => deleteConnectorsOf(page, TYPE);

/** The company's application as the API records it, and the whole listing. */
export async function stored(page: Page): Promise<{ app?: ConnectorView; listing: string }> {
  const { connector, listing } = await storedOf(page, TYPE);
  return { app: connector, listing };
}

/** The person's link at the application, as the API lists it to them. */
export const linkOf = (page: Page) => linkOfType(page, TYPE);

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
