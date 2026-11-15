// The company's standard mail in the end-to-end tests (ADR-205): declared
// from « Connecteurs », read back from the API, and deleted after each
// test with the accounts linked there.
import type { Locator, Page } from '@playwright/test';
import type { ConnectorView, Connectors } from '../../lib/api/contract';
import { cardWith, openAdmin } from './admin';
import { expect } from './fixtures';

const TYPE = 'mail_server';
const WRITE = { 'X-Mimorya-Request': '1' };

/** The label of the option that lets its tools write. */
export const WRITING = 'Permettre l’envoi d’e-mails et la création de rendez-vous, chacun validé';

/** Deletes the standard mail a test declared, with its linked accounts. */
export async function deleteMailServers(page: Page) {
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

/** The company's standard mail as the API records it. */
export async function stored(page: Page): Promise<ConnectorView | undefined> {
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  return instances.find((c) => c.type === TYPE);
}

/** Declares the standard mail from « Connecteurs », its IMAP server on 993
 * and its SMTP server on 465, neither agenda nor writing; returns its card. */
export async function declare(page: Page, name: string, imap: string, smtp: string) {
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Déclarer la messagerie standard' }).click();
  const form = page.getByRole('form', { name: 'Messagerie standard' });
  await form.getByLabel('Nom', { exact: true }).fill(name);
  await form.getByLabel('Serveur IMAP (TLS dès la connexion)').fill(imap);
  await form.getByLabel('Serveur SMTP').fill(smtp);
  await form.getByRole('button', { name: 'Déclarer la messagerie' }).click();
  const card: Locator = cardWith(page, name, 'Supprimer');
  await expect(card.getByText('Aucun agenda déclaré')).toBeVisible();
  return card;
}
